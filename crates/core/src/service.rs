//! The app's logic for one game's collection, shared by the desktop, phone and web
//! front ends: opening its files, refreshing prices, price history, the daily sync,
//! lots and sales. Importing is in `import`; the value chart in `collection`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use anyhow::{Result, anyhow, bail};

use crate::collection::Purchase;
use crate::collection::{Card, PriceHistory, SetInfo, Store};
use crate::collection::{lot_tag, same_tags, user_tags};
use crate::game::{Game, ImportReport};
use crate::lists::{List, ListEntry};
use crate::lots::{Lot, LotCopy, LotPrices, LotSplit, lot_prices};
use crate::records::{DataFile, Records};
use crate::sales::{CardRef, Sale};
use crate::sealed::SealedItem;
use crate::settings::{Settings, SettingsStore};
use crate::watchlist::Watch;
use crate::{datadir, dates};

/// How often running apps check whether prices need refreshing. Prices refresh at most
/// once a day, when the stored ones are from an earlier day.
pub const PRICE_CHECK_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// Told about a watch that triggered: the game's ID and the watch.
pub type AlertHook = Box<dyn Fn(&str, &Watch) + Send + Sync>;

/// One game with its open collection and price history.
pub struct GameService {
    pub game: Box<dyn Game>,
    pub store: Store,
    pub prices: PriceHistory,
    /// Lots: cards bought together for one price.
    pub lots: Records<Lot>,
    /// Sold cards.
    pub sales: Records<Sale>,
    /// Lists and decks.
    pub lists: Records<List>,
    /// Watched prices.
    pub watchlist: Records<Watch>,
    /// Sealed product.
    pub sealed: Records<SealedItem>,
    /// Told about each watch that triggers (the apps post a system notification).
    alert_hook: OnceLock<AlertHook>,
    /// The app settings (price refresh frequency, automatic price history). Without
    /// them, prices refresh daily and price history loads automatically.
    pub settings: Option<Arc<SettingsStore>>,
    /// Whether price history may load without being asked (the setting permitting).
    /// Phones turn it off: the download is about 50 MB, maybe over a cellular network.
    pub auto_history: bool,
    sets: Mutex<Option<Arc<HashMap<String, SetInfo>>>>,
}

impl GameService {
    /// Prepares the game's folders and opens its collection and price history from
    /// the `data` folder. The game keeps its caches in the `local` folder.
    pub fn open(local: &Path, data: &Path, game: Box<dyn Game>) -> Result<GameService> {
        game.open(&datadir::game_dir(local, game.id())?)?;
        let dir = datadir::game_dir(data, game.id())?;
        let store = Store::open(dir.join(datadir::COLLECTION_FILE), game.id())?;
        tracing::info!(game = game.id(), path = %store.file_path().display(), cards = store.len(), "collection opened");
        GameService::from_store(game, store)
    }

    /// Wraps an open store, loading the price history and lots kept next to it.
    pub fn from_store(game: Box<dyn Game>, store: Store) -> Result<GameService> {
        let dir = store.file_path().parent().unwrap_or(Path::new(".")).to_path_buf();
        let lots = Records::open(&dir)?;
        let sales = Records::open(&dir)?;
        let lists = Records::open(&dir)?;
        let watchlist = Records::open(&dir)?;
        let sealed = Records::open(&dir)?;
        let path = store.file_path().with_file_name(datadir::PRICE_HISTORY_FILE);
        let prices = match PriceHistory::open(&path) {
            Ok((prices, warning)) => {
                if let Some(w) = warning {
                    tracing::warn!(game = game.id(), "{w}");
                }
                prices
            }
            Err(e) => {
                // The file exists but couldn't be read or set aside: keep the history in
                // memory for this session and leave the file untouched.
                tracing::warn!(game = game.id(), error = %e, "price history unavailable");
                PriceHistory::in_memory()
            }
        };
        Ok(GameService {
            game,
            store,
            prices,
            lots,
            sales,
            lists,
            watchlist,
            sealed,
            alert_hook: OnceLock::new(),
            settings: None,
            auto_history: true,
            sets: Mutex::default(),
        })
    }

    /// Sets what is told about each price alert (once; later calls are ignored).
    pub fn set_alert_hook(&self, hook: AlertHook) {
        let _ = self.alert_hook.set(hook);
    }

    /// A lot's copies, for working out its prices.
    pub fn lot_copies(&self, lot: &Lot) -> Vec<LotCopy> {
        self.store.lot_copies(&lot.id, |c| self.game.is_basic_land(c))
    }

    /// Works out a lot's prices and saves them to its cards.
    pub fn reprice_lot(&self, lot: &Lot) -> Result<LotPrices> {
        let prices = lot_prices(lot, &self.lot_copies(lot));
        self.store.reprice_lot(&lot.id, &prices, &lot.acquired_date)?;
        Ok(prices)
    }

    /// Saves a changed lot and reprices it. A finished value-split lot is split at
    /// today's prices: the lot's cards are refreshed first when any price is from an
    /// earlier day (or always, with `refresh`), and the lot records the day.
    pub fn save_lot(&self, mut lot: Lot, refresh: bool) -> Result<(Lot, LotPrices)> {
        if !lot.open && lot.split == LotSplit::ByValue {
            let today = dates::today_string();
            let cards = self.store.cards_with_tag(&lot_tag(&lot.id));
            if refresh || cards.iter().any(|c| c.prices_updated < today) {
                let ids: Vec<String> = cards.iter().map(|c| c.id.clone()).collect();
                match self.game.refresh(&ids, &mut |_, _| {}) {
                    Ok(updates) => {
                        self.store.update_catalog(&updates, &today)?;
                    }
                    Err(e) => {
                        tracing::warn!(game = self.game.id(), error = %e, "could not refresh the lot's prices; using the stored ones")
                    }
                }
            }
            lot.split_date = today;
        } else {
            lot.split_date.clear();
        }
        self.lots.save(lot.clone())?;
        let prices = self.reprice_lot(&lot)?;
        Ok((lot, prices))
    }

    /// Sells copies from one purchase of the card stored under `key` (the purchase is
    /// `sale`'s finish and condition with `purchase`'s price paid, acquired date and user
    /// tags; the lot isn't compared). The sale is recorded first, then the copies leave the collection: if
    /// the second step fails, the sale exists and the copies remain, both visible.
    pub fn sell(&self, key: &str, purchase: &Purchase, mut sale: Sale) -> Result<Sale> {
        let Some(card) = self.store.get(key) else { bail!("that card isn't in the collection") };
        let found = card.purchases(&sale.finish, sale.condition).into_iter().find(|p| {
            p.price_paid == purchase.price_paid
                && p.acquired_date == purchase.acquired_date
                && same_tags(&user_tags(&p.tags).cloned().collect::<Vec<_>>(), &user_tags(&purchase.tags).cloned().collect::<Vec<_>>())
        });
        let Some(p) = found else { bail!("that purchase isn't in the collection any more") };
        if sale.quantity > p.quantity {
            bail!("there are only {} copies in that purchase", p.quantity);
        }
        sale.card = CardRef::from(&card);
        sale.cost_each = p.price_paid;
        sale.acquired_date = p.acquired_date.clone();
        sale.purchase_tags = p.tags.clone();
        let sale = self.sales.add(sale)?;
        if let Err(e) = self.store.take_copies(key, &sale.finish, sale.condition, &p, sale.quantity) {
            tracing::warn!(game = self.game.id(), sale = sale.id, error = %e, "sale recorded, but the copies stayed in the collection");
            bail!("the sale was recorded, but the copies couldn't be taken out of the collection: {e:#}");
        }
        tracing::info!(game = self.game.id(), card = sale.card.name, quantity = sale.quantity, "copies sold");
        Ok(sale)
    }

    /// Deletes a sale. With `restore`, its copies go back into the collection as the
    /// purchase they were (rebuilding the card from the sale if it has left, with its
    /// catalog data refreshed when the catalog can be reached).
    pub fn delete_sale(&self, id: &str, restore: bool) -> Result<Sale> {
        let Some(sale) = self.sales.get(id) else { bail!("there is no sale {id:?}") };
        if restore {
            let purchase = Purchase {
                quantity: sale.quantity,
                price_paid: sale.cost_each,
                acquired_date: sale.acquired_date.clone(),
                tags: sale.purchase_tags.clone(),
            };
            let existing = self.store.cards_where(|c| c.id == sale.card.id);
            let mut card = match existing.into_iter().next() {
                Some(c) => {
                    let mut c = Card { copies: Default::default(), ..c };
                    c.add_purchase(&sale.finish, sale.condition, purchase);
                    c
                }
                None => {
                    let mut c = sale.card.to_card();
                    c.add_purchase(&sale.finish, sale.condition, purchase);
                    c
                }
            };
            let gone = self.store.get(&crate::collection::card_key(&card)).is_none();
            if gone {
                // Prices, type line and rules text, if the catalog answers.
                match self.game.refresh(std::slice::from_ref(&card.id), &mut |_, _| {}) {
                    Ok(updates) => {
                        if let Some(u) = updates.get(&card.id) {
                            card.market = u.market.clone();
                            card.type_line = u.type_line.clone();
                            card.rules_text = u.rules_text.clone();
                            card.price_from = u.price_from.clone();
                            card.prices_updated = dates::today_string();
                        }
                    }
                    Err(e) => tracing::warn!(game = self.game.id(), error = %e, "restored card's catalog data not refreshed"),
                }
            }
            self.store.add_cards(vec![card])?;
        }
        self.sales.remove(id)?;
        tracing::info!(game = self.game.id(), card = sale.card.name, restore, "sale deleted");
        Ok(sale)
    }

    /// Reads a pasted card list (or any import file) as list entries. Lines matched
    /// by name only count any printing; a finish other than the usual one is kept.
    pub fn list_entries(&self, data: &[u8], progress: &mut dyn FnMut(usize, usize)) -> Result<(Vec<ListEntry>, ImportReport)> {
        let (cards, report) = self.game.import(data, progress)?;
        let usual = self.game.finishes().first().copied().unwrap_or("");
        let entries = cards
            .into_iter()
            .enumerate()
            .flat_map(|(i, c)| {
                let any = report.approximate.contains(&i);
                let mut by_finish: Vec<(String, i64)> = Vec::new();
                for (f, _, p) in c.copies.each() {
                    match by_finish.iter_mut().find(|(x, _)| x == f) {
                        Some((_, q)) => *q += p.quantity,
                        None => by_finish.push((f.to_string(), p.quantity)),
                    }
                }
                by_finish.into_iter().map(|(f, q)| ListEntry::new(c.clone(), q, any, if f == usual { "" } else { &f })).collect::<Vec<_>>()
            })
            .collect();
        Ok((entries, report))
    }

    /// The set sizes for completion stats, loaded once.
    pub fn sets(&self) -> Result<Arc<HashMap<String, SetInfo>>> {
        let mut cached = self.sets.lock().unwrap();
        if let Some(s) = &*cached {
            return Ok(s.clone());
        }
        let sets = Arc::new(self.game.sets()?);
        *cached = Some(sets.clone());
        Ok(sets)
    }

    /// The set sizes if already loaded, else empty.
    pub fn cached_sets(&self) -> Arc<HashMap<String, SetInfo>> {
        self.sets.lock().unwrap().clone().unwrap_or_default()
    }

    /// Every file the service keeps: the collection, the price history and the records
    /// files.
    pub fn files(&self) -> [&dyn DataFile; 7] {
        [&self.store, &self.prices, &self.lots, &self.sales, &self.lists, &self.watchlist, &self.sealed]
    }

    /// A number that changes whenever any of the service's files does.
    pub fn revision(&self) -> u64 {
        self.files().iter().map(|f| f.revision()).sum()
    }

    /// Takes in changes another device made to the service's files (through a synced
    /// folder). A file that is mid-sync is left for the next call.
    pub fn refresh_files(&self) {
        for f in self.files() {
            if let Err(e) = f.refresh() {
                tracing::debug!(game = self.game.id(), file = f.file_name(), error = %e, "file not refreshed");
            }
        }
    }

    /// Saves today's prices to the price history.
    pub fn record_today(&self) -> Result<()> {
        self.prices.record_current(&self.store.cards(), &dates::today_string());
        self.prices.save()
    }

    /// Prices the given cards from the game's catalog (only those still unpriced in the
    /// collection). A failure is logged: the next price refresh tries again.
    pub(crate) fn price_cards(&self, ids: &[String]) {
        let ids: Vec<String> = ids.iter().filter(|id| self.store.get(id).is_some_and(|c| c.market.is_empty())).cloned().collect();
        let updated = self.game.refresh(&ids, &mut |_, _| {}).and_then(|u| self.store.update_catalog(&u, &dates::today_string()));
        match updated {
            Ok(n) => tracing::info!(game = self.game.id(), cards = n, "priced cards imported without a price"),
            Err(e) => tracing::warn!(game = self.game.id(), error = %e, "could not price the imported cards"),
        }
    }

    /// Fetches current prices from the game's catalog, saves them and records today's
    /// prices. Returns the number of cards updated.
    pub fn refresh_prices(&self, progress: &mut dyn FnMut(usize, usize)) -> Result<usize> {
        // The lists' cards too, in the same request.
        let mut ids = self.store.ids();
        ids.extend(self.lists.ids());
        ids.extend(self.watchlist.ids());
        ids.sort();
        ids.dedup();
        let updates = self.game.refresh(&ids, progress)?;
        let today = dates::today_string();
        let n = self.store.update_catalog(&updates, &today)?;
        self.lists.update_catalog(&updates, &today)?;
        for w in self.watchlist.apply_prices(&updates, &today)? {
            tracing::info!(game = self.game.id(), card = w.card.name, finish = w.finish, price = w.price(), "price alert");
            if let Some(hook) = self.alert_hook.get() {
                hook(self.game.id(), &w);
            }
        }
        self.prices.record_current(&self.store.cards(), &today);
        self.prices.save()?;
        Ok(n)
    }

    /// Merges the game's daily price history (keeping prices already recorded) and
    /// returns how many printings had history.
    pub fn load_price_history(&self, status: &mut dyn FnMut(&str)) -> Result<usize> {
        let src = self.game.price_history().ok_or_else(|| anyhow!("this game has no price history source"))?;
        let cards = self.store.cards();
        if cards.is_empty() {
            return Ok(0);
        }
        let prices = src.load(&cards, status)?;
        for (id, by_finish) in &prices {
            self.prices.merge(id, by_finish);
        }
        self.prices.mark_history_loaded(&dates::today_string());
        self.prices.save()?;
        Ok(prices.len())
    }

    fn current_settings(&self) -> Settings {
        self.settings.as_ref().map(|s| s.get()).unwrap_or_default()
    }

    /// Whether automatic updates should refresh prices now: they are from an earlier day
    /// (daily), a week or more old (weekly), or never (manual).
    pub fn prices_due(&self) -> bool {
        match self.current_settings().price_refresh.as_str() {
            "manual" => false,
            "weekly" => self.store.prices_older_than(7),
            _ => self.store.prices_stale(),
        }
    }

    /// Whether to load price history without being asked: the game offers it, it was
    /// never loaded, there are cards, and the setting allows it.
    pub fn auto_load_history_due(&self) -> bool {
        self.game.price_history().is_some()
            && self.auto_history
            && self.current_settings().auto_price_history
            && self.prices.history_loaded_on().is_empty()
            && !self.store.is_empty()
    }

    /// Refreshes prices if they are due, and says whether they were.
    fn refresh_if_due(&self) -> bool {
        let due = self.prices_due();
        if due {
            match self.refresh_prices(&mut |_, _| {}) {
                Ok(n) => tracing::info!(game = self.game.id(), cards = n, "prices refreshed"),
                Err(e) => tracing::warn!(game = self.game.id(), error = %e, "price refresh failed"),
            }
        }
        due
    }

    /// The startup routine: load price history the first time (when allowed), refresh
    /// prices when due, and record today's prices.
    pub fn daily_sync(&self) {
        let id = self.game.id();
        if self.auto_load_history_due() {
            match self.load_price_history(&mut |_| {}) {
                Ok(n) => tracing::info!(game = id, printings = n, "price history loaded"),
                Err(e) => tracing::warn!(game = id, error = %e, "loading price history failed"),
            }
        }
        if !self.refresh_if_due()
            && let Err(e) = self.record_today()
        {
            tracing::warn!(game = id, error = %e, "could not record today's prices");
        }
    }

    /// Runs the daily sync, then refreshes prices whenever they are due, checking every
    /// [`PRICE_CHECK_INTERVAL`], on a background thread for the life of the app. Set
    /// sizes are loaded too.
    pub fn start_background_sync(self: &Arc<Self>) {
        let svc = self.clone();
        std::thread::Builder::new()
            .name(format!("{}-sets", svc.game.id()))
            .spawn(move || {
                if let Err(e) = svc.sets() {
                    tracing::warn!(game = svc.game.id(), error = %e, "could not load set list");
                }
            })
            .ok();
        let svc = self.clone();
        std::thread::Builder::new()
            .name(format!("{}-prices", svc.game.id()))
            .spawn(move || {
                svc.daily_sync();
                loop {
                    std::thread::sleep(PRICE_CHECK_INTERVAL);
                    svc.refresh_if_due();
                }
            })
            .ok();
    }
}
