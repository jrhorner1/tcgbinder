//! The app's logic for one game's collection, shared by the desktop, phone and web
//! front ends: opening its files, importing, refreshing prices, price history, the
//! daily sync, and the value chart.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use chrono::{Months, NaiveDate};

use crate::collection::Purchase;
use crate::collection::{Card, PriceHistory, SetInfo, Stats, Store, TimePoint, read_export};
use crate::game::{Game, ImportReport};
use crate::lists::{ListEntry, ListsStore};
use crate::lots::{Lot, LotCopy, LotPrices, LotSplit, LotsStore, lot_prices};
use crate::sales::{CardRef, Sale, SalesStore};
use crate::sealed::SealedStore;
use crate::settings::{Settings, SettingsStore};
use crate::watchlist::{Watch, WatchStore};
use crate::{datadir, dates};

/// How often running apps check whether prices need refreshing. Prices refresh at most
/// once a day, when the stored ones are from an earlier day.
pub const PRICE_CHECK_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// Names collection files written by this app in import reports.
pub const APP_FILE_FORMAT: &str = "TCG Binder collection file";

/// Told about a watch that triggered: the game's ID and the watch.
pub type AlertHook = Box<dyn Fn(&str, &Watch) + Send + Sync>;

/// One game with its open collection and price history.
pub struct GameService {
    pub game: Box<dyn Game>,
    pub store: Store,
    pub prices: PriceHistory,
    /// Lots: cards bought together for one price.
    pub lots: LotsStore,
    /// Sold cards.
    pub sales: SalesStore,
    /// Lists and decks.
    pub lists: ListsStore,
    /// Watched prices.
    pub watchlist: WatchStore,
    /// Sealed product.
    pub sealed: SealedStore,
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
        let lots = LotsStore::open(&dir)?;
        let sales = SalesStore::open(&dir)?;
        let lists = ListsStore::open(&dir)?;
        let watchlist = WatchStore::open(&dir)?;
        let sealed = SealedStore::open(&dir)?;
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
            let cards = self.store.lot_cards(&lot.id);
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
        self.lots.save(&lot)?;
        let prices = self.reprice_lot(&lot)?;
        Ok((lot, prices))
    }

    /// Sells copies from one purchase of the card stored under `key` (the purchase is
    /// `sale`'s finish and condition with `purchase`'s price paid, acquired date and
    /// note). The sale is recorded first, then the copies leave the collection: if
    /// the second step fails, the sale exists and the copies remain, both visible.
    pub fn sell(&self, key: &str, purchase: &Purchase, mut sale: Sale) -> Result<Sale> {
        let Some(card) = self.store.get(key) else { bail!("that card isn't in the collection") };
        let found = card
            .purchases(&sale.finish, sale.condition)
            .into_iter()
            .find(|p| p.price_paid == purchase.price_paid && p.acquired_date == purchase.acquired_date && p.note == purchase.note);
        let Some(p) = found else { bail!("that purchase isn't in the collection any more") };
        if sale.quantity > p.quantity {
            bail!("there are only {} copies in that purchase", p.quantity);
        }
        sale.card = CardRef::from(&card);
        sale.cost_each = p.price_paid;
        sale.acquired_date = p.acquired_date.clone();
        sale.purchase_note = p.note.clone();
        sale.lot = p.lot.clone();
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
                note: sale.purchase_note.clone(),
                lot: sale.lot.clone(),
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

    /// Takes in changes another device made to the collection and price history files
    /// (through a synced folder). A file that is mid-sync is left for the next call.
    pub fn refresh_files(&self) {
        if let Err(e) = self.store.refresh() {
            tracing::debug!(game = self.game.id(), error = %e, "collection not refreshed");
        }
        if let Err(e) = self.prices.refresh() {
            tracing::debug!(game = self.game.id(), error = %e, "price history not refreshed");
        }
        if let Err(e) = self.sealed.refresh() {
            tracing::debug!(game = self.game.id(), error = %e, "sealed product not refreshed");
        }
        if let Err(e) = self.watchlist.refresh() {
            tracing::debug!(game = self.game.id(), error = %e, "watchlist not refreshed");
        }
        if let Err(e) = self.lists.refresh() {
            tracing::debug!(game = self.game.id(), error = %e, "lists not refreshed");
        }
        if let Err(e) = self.sales.refresh() {
            tracing::debug!(game = self.game.id(), error = %e, "sales not refreshed");
        }
        if let Err(e) = self.lots.refresh() {
            tracing::debug!(game = self.game.id(), error = %e, "lots not refreshed");
        }
    }

    /// Saves today's prices to the price history.
    pub fn record_today(&self) -> Result<()> {
        self.prices.record_current(&self.store.cards(), &dates::today_string());
        self.prices.save()
    }

    /// Prices the given cards from the game's catalog (only those still unpriced in the
    /// collection). A failure is logged: the next price refresh tries again.
    fn price_cards(&self, ids: &[String]) {
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

    /// Adds a file's cards: a CSV export from another app, or a collection file written
    /// by this app. Cards for a printing already owned add to its quantity.
    pub fn import(&self, data: &[u8], progress: &mut dyn FnMut(usize, usize)) -> Result<ImportResult> {
        self.commit_import(self.preview_import(data, progress)?)
    }

    /// Reads and matches a file's cards without saving anything, and says what
    /// importing them would do.
    pub fn preview_import(&self, data: &[u8], progress: &mut dyn FnMut(usize, usize)) -> Result<ImportPreview> {
        let (cards, report) = parse_import(self.game.as_ref(), data, progress)?;
        let (new_cards, already_owned) = self.store.count_new(&cards);
        let copies = cards.iter().map(Card::copy_count).sum();
        let price_paid = cards.iter().map(|c| c.price_paid().0).sum();
        Ok(ImportPreview { cards, report, copies, new_cards, already_owned, price_paid })
    }

    /// Saves the cards of a preview. The counts are worked out again, since the
    /// collection may have changed since the preview. Cards that came in without a
    /// price (such as non-English printings) are then priced, so they don't wait for
    /// the next refresh.
    pub fn commit_import(&self, preview: ImportPreview) -> Result<ImportResult> {
        let ImportPreview { cards, report, copies, .. } = preview;
        let unpriced: Vec<String> = cards.iter().filter(|c| c.market.is_empty()).map(|c| c.id.clone()).collect();
        let backup = self.store.backup("import")?;
        let (new_cards, already_owned) = self.store.add_cards(cards)?;
        if !unpriced.is_empty() {
            self.price_cards(&unpriced);
        }
        tracing::info!(
            game = self.game.id(),
            format = report.format,
            cards = copies,
            new = new_cards,
            merged = already_owned,
            "import complete"
        );
        Ok(ImportResult { report, cards: copies, new_cards, already_owned, backup })
    }
}

/// What importing a file would do, with the cards it would add.
#[derive(Clone, Debug, Default)]
pub struct ImportPreview {
    pub cards: Vec<Card>,
    pub report: ImportReport,
    /// Copies to import.
    pub copies: i64,
    /// Cards (printings) not yet owned.
    pub new_cards: usize,
    /// Cards that would add copies to ones already owned.
    pub already_owned: usize,
    /// The price paid for every copy with a known price.
    pub price_paid: f64,
}

impl ImportPreview {
    /// The number of distinct printings to import.
    pub fn printings(&self) -> usize {
        self.cards.iter().map(|c| c.id.as_str()).collect::<std::collections::HashSet<_>>().len()
    }
}

/// A summary of an import.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImportResult {
    pub report: ImportReport,
    /// Copies imported.
    pub cards: i64,
    pub new_cards: usize,
    pub already_owned: usize,
    /// The backup of the collection taken just before (its name), for undo.
    pub backup: String,
}

impl ImportResult {
    /// The result for people.
    pub fn summary(&self) -> String {
        let mut b = String::new();
        let _ = writeln!(b, "Format: {}", self.report.format);
        let read = if self.report.format == APP_FILE_FORMAT { "Cards read" } else { "Rows read" };
        let _ = writeln!(b, "{read}: {}", self.report.rows);
        let _ = writeln!(
            b,
            "Cards imported: {} ({} new cards, {} added to cards you already own)",
            self.cards, self.new_cards, self.already_owned
        );
        if !self.report.notes.is_empty() {
            b.push('\n');
        }
        for n in &self.report.notes {
            let _ = writeln!(b, "{n}");
        }
        b
    }
}

/// Whether `data` looks like a JSON collection file rather than CSV.
pub fn is_app_file(data: &[u8]) -> bool {
    let data = data.strip_prefix("\u{feff}".as_bytes()).unwrap_or(data);
    data.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'{')
}

/// Turns a file into cards for `game` without saving them. App collection files already
/// carry exact catalog IDs and prices, so they are used as-is; anything else goes to
/// the game's CSV importer.
pub fn parse_import(game: &dyn Game, data: &[u8], progress: &mut dyn FnMut(usize, usize)) -> Result<(Vec<Card>, ImportReport)> {
    if !is_app_file(data) {
        return game.import(data, progress);
    }
    let (file_game, cards) = read_export(data)?;
    if file_game != game.id() {
        bail!("this file is a {file_game:?} collection; it can't be imported into {}", game.name());
    }
    if cards.is_empty() {
        bail!("no cards found in this collection file");
    }
    let report = ImportReport { format: APP_FILE_FORMAT.into(), rows: cards.len(), ..Default::default() };
    Ok((cards, report))
}

// --- The value chart --------------------------------------------------------------

/// The Value Over Time range shown first (see [`crate::settings::choices::CHART_RANGES`]), matching the 90 days of history MTGJSON provides.
pub const DEFAULT_CHART_RANGE: &str = "90D";

/// The first day shown for a range; `None` means everything.
pub fn range_start(range: &str, today: NaiveDate) -> Option<NaiveDate> {
    match range {
        "30D" => Some(today - chrono::Duration::days(30)),
        "90D" => Some(today - chrono::Duration::days(90)),
        "1Y" => today.checked_sub_months(Months::new(12)),
        _ => None,
    }
}

/// The data behind the Value Over Time chart.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ValueChart {
    /// `None` for "All".
    pub from: Option<NaiveDate>,
    pub to: NaiveDate,
    /// The first day with recorded prices.
    pub history_start: Option<NaiveDate>,
    /// The real daily market value.
    pub market_value: Vec<TimePoint>,
    /// The running price paid (a step line).
    pub price_paid: Vec<TimePoint>,
    /// Values cards at today's prices before recorded prices begin (a step line); empty
    /// when the real line covers the whole range.
    pub estimate: Vec<TimePoint>,
}

/// Computes the chart for `cards` over the named range.
pub fn build_value_chart(cards: &[Card], st: &Stats, prices: &PriceHistory, range: &str, today: NaiveDate) -> ValueChart {
    let from = range_start(range, today);
    let history_start = prices.start();
    let mut vc = ValueChart { from, to: today, history_start, ..Default::default() };
    if let Some(market_from) = from.or(history_start) {
        vc.market_value = prices.market_series(cards, market_from, today);
    }
    vc.price_paid = clip_step(&st.price_paid_history, from, today);
    let before_history = match (history_start, from) {
        (Some(h), Some(f)) => f < h,
        _ => true,
    };
    if before_history {
        let est = clip_step(&st.value_history, from, today);
        vc.estimate = match history_start {
            Some(h) => end_at(&est, h), // real prices take over from here
            None => est,
        };
    }
    vc
}

/// Limits a step series to dates on or after `from`, starting with the value in effect
/// on that day so the line doesn't begin at zero mid-range.
pub fn clip_step(points: &[TimePoint], from: Option<NaiveDate>, today: NaiveDate) -> Vec<TimePoint> {
    let Some(from) = from else { return points.to_vec() };
    let mut out = Vec::new();
    for (i, p) in points.iter().enumerate() {
        if p.date > from {
            if out.is_empty() && i > 0 {
                out.push(TimePoint { date: from, value: points[i - 1].value });
            }
            out.push(*p);
        } else if p.date == from {
            out.push(*p);
        }
    }
    if let (true, Some(last)) = (out.is_empty(), points.last()) {
        // Everything happened before the range.
        let end = if last.date > from { last.date } else { today };
        out = vec![TimePoint { date: from, value: last.value }, TimePoint { date: end, value: last.value }];
    }
    out
}

/// Keeps a step series' points before `until` and ends it with the value in effect on
/// that day.
pub fn end_at(points: &[TimePoint], until: NaiveDate) -> Vec<TimePoint> {
    let mut out: Vec<TimePoint> = points.iter().take_while(|p| p.date < until).copied().collect();
    if let Some(last) = out.last().copied() {
        out.push(TimePoint { date: until, value: last.value });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::{Condition, Purchase, compute_stats_at};

    fn day(s: &str) -> NaiveDate {
        dates::parse(s).unwrap()
    }

    fn pts(v: &[TimePoint]) -> Vec<(String, f64)> {
        v.iter().map(|p| (dates::format(p.date), p.value)).collect()
    }

    fn tp(d: &str, v: f64) -> TimePoint {
        TimePoint { date: day(d), value: v }
    }

    #[test]
    fn ranges() {
        let t = day("2024-03-31");
        assert_eq!(range_start("30D", t), Some(day("2024-03-01")));
        assert_eq!(range_start("1Y", t), Some(day("2023-03-31")));
        assert_eq!(range_start("All", t), None);
    }

    #[test]
    fn clip_and_end() {
        let s = [tp("2024-01-01", 1.0), tp("2024-02-01", 2.0), tp("2024-03-01", 3.0)];
        let today = day("2024-04-01");
        assert_eq!(
            pts(&clip_step(&s, Some(day("2024-01-15")), today)),
            [("2024-01-15".into(), 1.0), ("2024-02-01".into(), 2.0), ("2024-03-01".into(), 3.0)]
        );
        assert_eq!(pts(&clip_step(&s, Some(day("2024-03-15")), today)), [("2024-03-15".into(), 3.0), ("2024-04-01".into(), 3.0)]);
        assert_eq!(clip_step(&s, None, today).len(), 3);
        assert_eq!(
            pts(&end_at(&s, day("2024-02-15"))),
            [("2024-01-01".into(), 1.0), ("2024-02-01".into(), 2.0), ("2024-02-15".into(), 2.0)]
        );
        assert!(end_at(&s, day("2023-01-01")).is_empty());
    }

    #[test]
    fn chart_estimates_before_history() {
        let mut c = Card { id: "a".into(), ..Card::default() };
        c.add_purchase(
            "nonfoil",
            Condition::NearMint,
            Purchase { quantity: 1, price_paid: 2.0, acquired_date: "2024-01-01".into(), ..Default::default() },
        );
        c.market.insert("nonfoil".into(), 5.0);
        let cards = vec![c];
        let today = day("2024-03-01");
        let prices = PriceHistory::in_memory();
        prices.set("a", "nonfoil", "2024-02-20", 4.0);
        let st = compute_stats_at(&cards, &HashMap::new(), today);
        let vc = build_value_chart(&cards, &st, &prices, "30D", today);
        assert_eq!(vc.history_start, Some(day("2024-02-20")));
        assert_eq!(pts(&vc.market_value), [("2024-02-20".into(), 4.0), ("2024-03-01".into(), 4.0)]);
        assert_eq!(pts(&vc.estimate), [("2024-01-31".into(), 5.0), ("2024-02-20".into(), 5.0)]);
        assert_eq!(pts(&vc.price_paid), [("2024-01-31".into(), 2.0), ("2024-03-01".into(), 2.0)]);

        let all = build_value_chart(&cards, &st, &PriceHistory::in_memory(), "All", today);
        assert!(all.market_value.is_empty());
        assert_eq!(all.estimate.len(), 2);
    }

    #[test]
    fn app_files() {
        assert!(is_app_file(b"\xef\xbb\xbf  {\"cards\":{}}"));
        assert!(!is_app_file(b"Name,Qty"));
        let r = ImportResult {
            report: ImportReport { format: APP_FILE_FORMAT.into(), rows: 2, notes: vec!["note".into()], ..Default::default() },
            cards: 3,
            new_cards: 1,
            already_owned: 1,
            ..Default::default()
        };
        assert_eq!(
            r.summary(),
            "Format: TCG Binder collection file\nCards read: 2\nCards imported: 3 (1 new cards, 1 added to cards you already own)\n\nnote\n"
        );
    }
}
