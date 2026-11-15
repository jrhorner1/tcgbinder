use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use super::tags::{clean_tag, normalize_tags, same_tag, same_tags, tags_in_use, with_spellings};
use super::{BulkChange, BulkResult, Card, CatalogUpdate, Condition, Purchase};
use crate::dates;
use crate::jsonfile::JsonFile;
use crate::lots::{LotCopy, LotPrices};

/// One game's collection, persisted as a JSON file. Each game has its own. The file
/// may also be changed by another device through a synced folder: every change first
/// takes in what is on disk (see [`JsonFile`]).
pub struct Store {
    file: JsonFile<StoreData>,
}

/// A card to put in a lot ([`Store::put_in_lot`]): all its copies not in the lot, or
/// `quantity` of them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LotPick {
    pub key: String,
    pub quantity: Option<i64>,
}

/// What renaming or deleting a tag did: purchases changed, and the backup taken
/// first (None when the tag wasn't in use, so nothing was done).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TagEdit {
    pub changed: usize,
    pub backup: Option<String>,
}

/// The collection file's contents.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct StoreData {
    /// The ID of the game this file belongs to.
    #[serde(skip_serializing_if = "String::is_empty")]
    game: String,
    cards: BTreeMap<String, Card>,
}

/// How many backups of the collection are kept.
pub const BACKUPS_KEPT: usize = 10;

/// Why a backup was taken, as the last part of its file name.
pub const BACKUP_REASONS: &[&str] = &["import", "bulk-edit", "bulk-delete", "restore", "merge", "move", "tag-rename", "tag-delete"];

/// A backup of the collection file, as listed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BackupInfo {
    /// The file name, which identifies the backup.
    pub name: String,
    /// When it was taken, `YYYY-MM-DD HH:MM:SS` (local time).
    pub date: String,
    /// Why it was taken (one of [`BACKUP_REASONS`]).
    pub reason: String,
    /// Cards (printings) in it.
    pub cards: usize,
    /// Its size in bytes.
    pub size: u64,
}

/// Reads a backup's name: `collection-YYYYMMDD-HHMMSS-<reason>.json`, possibly with
/// `-N` after the time when more were taken in the same second. Returns the date, N
/// (1 without it) and the reason.
fn parse_backup_name(name: &str) -> Option<(String, u32, String)> {
    let rest = name.strip_prefix("collection-")?.strip_suffix(".json")?;
    let (day, rest) = rest.split_once('-')?;
    let (time, rest) = rest.split_once('-')?;
    let (seq, reason) = match rest.split_once('-') {
        Some((n, reason)) if n.bytes().all(|b| b.is_ascii_digit()) => (n.parse().ok()?, reason),
        _ => (1, rest),
    };
    let stamp = chrono::NaiveDateTime::parse_from_str(&format!("{day}{time}"), "%Y%m%d%H%M%S").ok()?;
    BACKUP_REASONS.contains(&reason).then(|| (stamp.format("%Y-%m-%d %H:%M:%S").to_string(), seq, reason.to_string()))
}

/// The key of a collection card: one per printing, with its copies grouped by finish
/// and condition.
pub fn card_key(c: &Card) -> String {
    c.id.clone()
}

impl Store {
    /// Opens (or creates) the collection file for `game` at `path`. A file that belongs
    /// to a different game is refused; a new file is claimed for this one.
    pub fn open(path: impl AsRef<Path>, game: &str) -> Result<Store> {
        let file = JsonFile::open(path, true).context("failed to load existing collection")?;
        let store = Store { file };
        let owner = store.file.read(|d| d.game.clone());
        if owner.is_empty() {
            store.file.mutate(|d| {
                d.game = game.to_string();
                Ok(())
            })?;
        } else if owner != game {
            bail!("{} holds a {owner:?} collection, not {game:?}", store.file_path().display());
        }
        Ok(store)
    }

    /// The absolute path of the collection file.
    pub fn file_path(&self) -> &Path {
        self.file.path()
    }

    /// A number that changes whenever the collection does.
    pub fn revision(&self) -> u64 {
        self.file.revision()
    }

    /// The folder backups are kept in: `backups` next to the collection file.
    pub fn backup_dir(&self) -> PathBuf {
        self.file_path().with_file_name("backups")
    }

    /// Copies the collection file to the backups folder as
    /// `collection-YYYYMMDD-HHMMSS-<reason>.json` and keeps the [`BACKUPS_KEPT`] newest.
    /// Returns the backup's name.
    pub fn backup(&self, reason: &str) -> Result<String> {
        if !BACKUP_REASONS.contains(&reason) {
            bail!("unknown backup reason {reason:?}");
        }
        let dir = self.backup_dir();
        fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
        // Backups taken in the same second are numbered in order: -2-, -3-, ...
        let prefix = format!("collection-{stamp}-");
        let last = self
            .backup_names()?
            .iter()
            .filter(|n| n.starts_with(&prefix))
            .filter_map(|n| parse_backup_name(n))
            .map(|(_, seq, _)| seq)
            .max();
        let name = match last {
            None => format!("{prefix}{reason}.json"),
            Some(seq) => format!("{prefix}{}-{reason}.json", seq + 1),
        };
        let bytes = fs::read(self.file_path()).context("reading the collection to back it up")?;
        crate::jsonfile::write_atomic(&dir.join(&name), "backup_", &bytes).context("saving a backup")?;
        let mut names = self.backup_names()?;
        while names.len() > BACKUPS_KEPT {
            let old = names.remove(0);
            if let Err(e) = fs::remove_file(dir.join(&old)) {
                tracing::warn!(backup = old, error = %e, "could not delete an old backup");
            }
        }
        tracing::info!(backup = name, "collection backed up");
        Ok(name)
    }

    /// The backups' file names, oldest first.
    fn backup_names(&self) -> Result<Vec<String>> {
        let entries = match fs::read_dir(self.backup_dir()) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e).context("listing backups"),
        };
        let mut names: Vec<(String, u32, String)> = entries
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter_map(|n| parse_backup_name(&n).map(|(date, seq, _)| (date, seq, n)))
            .collect();
        names.sort();
        Ok(names.into_iter().map(|(_, _, n)| n).collect())
    }

    /// The backups, newest first.
    pub fn backups(&self) -> Result<Vec<BackupInfo>> {
        let dir = self.backup_dir();
        let mut out = Vec::new();
        for name in self.backup_names()?.into_iter().rev() {
            let Some((date, _, reason)) = parse_backup_name(&name) else { continue };
            let path = dir.join(&name);
            let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            let cards = fs::read(&path).ok().and_then(|b| serde_json::from_slice::<StoreData>(&b).ok()).map_or(0, |d| d.cards.len());
            out.push(BackupInfo { name, date, reason, cards, size });
        }
        Ok(out)
    }

    /// Replaces the collection with a backup's, after backing up the current one
    /// (reason `restore`). `name` must be one of the listed backups. Returns the name
    /// of the backup taken first.
    pub fn restore(&self, name: &str) -> Result<String> {
        if !self.backup_names()?.iter().any(|n| n == name) {
            bail!("there is no backup named {name:?}");
        }
        let bytes = fs::read(self.backup_dir().join(name)).context("reading the backup")?;
        let restored: StoreData = serde_json::from_slice(&bytes).context("the backup is damaged")?;
        let game = self.file.read(|d| d.game.clone());
        if !restored.game.is_empty() && restored.game != game {
            bail!("that backup is a {:?} collection, not {game:?}", restored.game);
        }
        let before = self.backup("restore")?;
        self.file.mutate(|d| {
            d.cards = restored.cards;
            Ok(())
        })?;
        tracing::info!(backup = name, "collection restored");
        Ok(before)
    }

    /// Takes in changes another device made to the file. True if there were any.
    pub fn refresh(&self) -> Result<bool> {
        self.file.refresh()
    }

    /// Adds copies to the collection, merging into the card for the same printing when
    /// there is one, and saves once. It returns how many new cards were created and how
    /// many cards were added to existing ones.
    pub fn add_cards(&self, mut cards: Vec<Card>) -> Result<(usize, usize)> {
        for c in &mut cards {
            normalize_card_tags(c)?;
        }
        self.file.mutate(|data| {
            let (mut created, mut merged) = (0, 0);
            for mut card in cards {
                if card.id.is_empty() || card.copy_count() <= 0 {
                    continue;
                }
                use_existing_spellings(data, &mut card);
                normalize_card(&mut card);
                let key = card_key(&card);
                let next = match data.cards.remove(&key) {
                    Some(existing) => {
                        merged += 1;
                        merge_cards(existing, card)
                    }
                    None => {
                        created += 1;
                        card
                    }
                };
                data.cards.insert(key, next);
            }
            Ok((created, merged))
        })
    }

    /// Merges the cards of a conflict copy of this collection (the same file, saved by
    /// another device at the same time), without counting anything twice: the union of
    /// both files' cards; for a card in both, the union of their purchases, taking the
    /// larger quantity where both have the same purchase (finish, condition, price paid,
    /// acquired date and note). A card or purchase deleted on one side comes back. Returns
    /// the number of cards that changed or were added.
    pub fn merge_copy(&self, cards: Vec<Card>) -> Result<usize> {
        self.file.mutate(|data| {
            let mut changed = 0;
            for mut theirs in cards.into_iter().filter(|c| !c.id.is_empty()) {
                normalize_card(&mut theirs);
                let key = card_key(&theirs);
                let Some(ours) = data.cards.get_mut(&key) else {
                    data.cards.insert(key, theirs);
                    changed += 1;
                    continue;
                };
                let before = ours.copies.clone();
                for (finish, cond, p) in theirs.copies.each() {
                    let list = ours.copies.0.entry(finish.to_string()).or_default().entry(cond).or_default();
                    match list.iter_mut().find(|q| q.same_as(p)) {
                        Some(q) => q.quantity = q.quantity.max(p.quantity),
                        None => list.push(p.clone()),
                    }
                }
                changed += usize::from(ours.copies != before);
            }
            Ok(changed)
        })
    }

    /// A lot's copies, one per card, finish and condition (purchases with different
    /// notes added together), for working out the lot's prices.
    pub fn lot_copies(&self, lot_id: &str, is_basic_land: impl Fn(&Card) -> bool) -> Vec<LotCopy> {
        self.file.read(|data| {
            let mut out: Vec<LotCopy> = Vec::new();
            for c in data.cards.values() {
                for (finish, condition, p) in c.copies.each().filter(|(_, _, p)| p.lot == lot_id) {
                    match out.iter_mut().find(|x| x.card_id == c.id && x.finish == finish && x.condition == condition) {
                        Some(x) => x.quantity += p.quantity,
                        None => out.push(LotCopy {
                            card_id: c.id.clone(),
                            finish: finish.to_string(),
                            condition,
                            quantity: p.quantity,
                            market: c.market_price(finish),
                            rarity: c.rarity.clone(),
                            basic_land: is_basic_land(c),
                        }),
                    }
                }
            }
            out
        })
    }

    /// The cards with copies in a lot.
    pub fn lot_cards(&self, lot_id: &str) -> Vec<Card> {
        self.cards_where(|c| c.copies.each().any(|(_, _, p)| p.lot == lot_id))
    }

    /// Sets the price paid (from `prices`) and acquired date of every copy in a lot, in
    /// one save. Returns the number of cards changed.
    pub fn reprice_lot(&self, lot_id: &str, prices: &LotPrices, acquired_date: &str) -> Result<usize> {
        self.file.mutate(|data| {
            let mut changed = 0;
            let mut extra = prices.extra.clone();
            for c in data.cards.values_mut().filter(|c| c.copies.each().any(|(_, _, p)| p.lot == lot_id)) {
                let mut copies = super::Copies::default();
                for (finish, cond, p) in c.copies.each() {
                    let mut p = p.clone();
                    if p.lot == lot_id {
                        let key = (c.id.clone(), finish.to_string(), cond);
                        if let Some(price) = prices.each.get(&key) {
                            p.price_paid = *price;
                        }
                        p.acquired_date = acquired_date.to_string();
                        // The rounding remainder goes on one copy of its card.
                        if let Some((_, more)) = extra.take_if(|(k, _)| *k == key) {
                            let one = super::Purchase { quantity: 1, price_paid: crate::lots::round4(p.price_paid + more), ..p.clone() };
                            p.quantity -= 1;
                            copies.add(finish, cond, one);
                        }
                    }
                    copies.add(finish, cond, p);
                }
                c.copies = copies;
                changed += 1;
            }
            Ok(changed)
        })
    }

    /// Deletes a lot from its cards: `remove` deletes the copies, otherwise they stay
    /// with their prices and no longer belong to a lot. Returns the cards changed.
    pub fn drop_lot(&self, lot_id: &str, remove: bool) -> Result<usize> {
        self.file.mutate(|data| {
            let mut changed = 0;
            for c in data.cards.values_mut().filter(|c| c.copies.each().any(|(_, _, p)| p.lot == lot_id)) {
                let mut copies = super::Copies::default();
                for (finish, cond, p) in c.copies.each() {
                    match (p.lot == lot_id, remove) {
                        (true, true) => {}
                        (true, false) => copies.add(finish, cond, super::Purchase { lot: String::new(), ..p.clone() }),
                        (false, _) => copies.add(finish, cond, p.clone()),
                    }
                }
                c.copies = copies;
                changed += 1;
            }
            data.cards.retain(|_, c| c.copy_count() > 0);
            Ok(changed)
        })
    }

    /// Moves copies the collection already holds into a lot, in one save: for each
    /// pick, every copy of the card not already in the lot, or `quantity` of them
    /// (copies in no lot first, then copies from other lots). Moved copies take the
    /// lot's date; their price comes from repricing the lot. Returns the copies moved
    /// and the other lots copies left, which need repricing too.
    pub fn put_in_lot(&self, lot_id: &str, acquired_date: &str, picks: &[LotPick]) -> Result<(i64, Vec<String>)> {
        self.file.mutate(|data| {
            let (mut moved, mut left) = (0, Vec::<String>::new());
            for pick in picks {
                let Some(card) = data.cards.get_mut(&pick.key) else { bail!("that card isn't in the collection") };
                let available: i64 = card.copies.each().filter(|(_, _, p)| p.lot != lot_id).map(|(_, _, p)| p.quantity).sum();
                let mut want = pick.quantity.unwrap_or(available);
                if want <= 0 || want > available {
                    bail!("{} has {} {} that could join the lot", card.name, available, if available == 1 { "copy" } else { "copies" });
                }
                // Copies in no lot first, then those in other lots.
                let mut rows = card.purchase_rows();
                rows.sort_by_key(|r| !r.purchase.lot.is_empty());
                let mut copies = super::Copies::default();
                for r in rows {
                    let p = r.purchase;
                    let take = if p.lot == lot_id { 0 } else { want.min(p.quantity) };
                    if take > 0 {
                        if !p.lot.is_empty() && !left.contains(&p.lot) {
                            left.push(p.lot.clone());
                        }
                        let joined =
                            Purchase { quantity: take, lot: lot_id.to_string(), acquired_date: acquired_date.to_string(), ..p.clone() };
                        copies.add(&r.finish, r.condition, joined);
                        want -= take;
                        moved += take;
                    }
                    copies.add(&r.finish, r.condition, Purchase { quantity: p.quantity - take, ..p });
                }
                card.copies = copies;
            }
            Ok((moved, left))
        })
    }

    /// Takes a card out of a lot: its copies stay in the collection with the price and
    /// date the lot gave them, in no lot. Returns the copies freed.
    pub fn remove_from_lot(&self, lot_id: &str, key: &str) -> Result<i64> {
        self.file.mutate(|data| {
            let Some(card) = data.cards.get_mut(key) else { bail!("that card isn't in the collection") };
            let mut freed = 0;
            let mut copies = super::Copies::default();
            for (f, cond, p) in card.copies.each() {
                if p.lot == lot_id {
                    freed += p.quantity;
                    copies.add(f, cond, Purchase { lot: String::new(), ..p.clone() });
                } else {
                    copies.add(f, cond, p.clone());
                }
            }
            if freed == 0 {
                bail!("{} isn't in that lot", card.name);
            }
            card.copies = copies;
            Ok(freed)
        })
    }

    /// What [`Store::add_cards`] would return for `cards`, without changing anything:
    /// how many new cards it would create and how many it would add to.
    pub fn count_new(&self, cards: &[Card]) -> (usize, usize) {
        self.file.read(|data| {
            let mut seen: HashSet<String> = HashSet::new();
            let (mut created, mut merged) = (0, 0);
            for card in cards.iter().filter(|c| !c.id.is_empty() && c.copy_count() > 0) {
                let key = card_key(card);
                if data.cards.contains_key(&key) || !seen.insert(key) {
                    merged += 1;
                } else {
                    created += 1;
                }
            }
            (created, merged)
        })
    }

    /// Saves an edited card that was stored under `old_key` (its copies replace the old
    /// ones). If it moved to another printing already in the collection, its copies
    /// merge into that card. No copies left deletes it. Returns the new key and whether
    /// it merged.
    pub fn save_card(&self, old_key: &str, mut card: Card) -> Result<(String, bool)> {
        if card.id.is_empty() {
            bail!("cannot save a card without a catalog ID");
        }
        if card.copies.0.values().flat_map(|m| m.values().flatten()).any(|p| p.quantity < 0) {
            bail!("quantity cannot be negative");
        }
        normalize_card_tags(&mut card)?;
        normalize_card(&mut card);
        let new_key = card_key(&card);
        self.file.mutate(|data| {
            let mut merged = false;
            data.cards.remove(old_key);
            // The spellings in the rest of the collection (this card's own may change).
            use_existing_spellings(data, &mut card);
            normalize_card(&mut card);
            if card.copy_count() > 0 {
                let next = match data.cards.remove(&new_key) {
                    Some(existing) if new_key != old_key => {
                        merged = true;
                        merge_cards(existing, card)
                    }
                    _ => card,
                };
                data.cards.insert(new_key.clone(), next);
            }
            Ok((new_key, merged))
        })
    }

    /// Drops the card with the given key (doing nothing if there is none).
    pub fn remove_card(&self, key: &str) -> Result<()> {
        if self.file.read(|d| !d.cards.contains_key(key)) {
            return Ok(());
        }
        self.file.mutate(|data| {
            data.cards.remove(key);
            Ok(())
        })
    }

    /// Takes `quantity` copies out of one purchase of the card stored under `key`: the
    /// one in `finish` and `cond` with `purchase`'s price paid, acquired date, note and
    /// tags.
    /// A card left with no copies is deleted. Returns the purchase as it was.
    pub fn take_copies(&self, key: &str, finish: &str, cond: Condition, purchase: &Purchase, quantity: i64) -> Result<Purchase> {
        if quantity <= 0 {
            bail!("take at least one copy");
        }
        let finish = super::copies::norm_finish(finish);
        self.file.mutate(|data| {
            let Some(card) = data.cards.get_mut(key) else { bail!("that card isn't in the collection") };
            let list = card.copies.0.get_mut(&finish).and_then(|m| m.get_mut(&cond));
            let found = list.and_then(|l| {
                l.iter_mut().find(|p| {
                    p.price_paid == purchase.price_paid
                        && p.acquired_date == purchase.acquired_date
                        && p.note == purchase.note
                        && same_tags(&p.tags, &purchase.tags)
                        && p.quantity > 0
                })
            });
            let Some(p) = found else { bail!("that purchase isn't in the collection any more") };
            if quantity > p.quantity {
                bail!("there are only {} copies in that purchase", p.quantity);
            }
            let before = p.clone();
            p.quantity -= quantity;
            normalize_card(card);
            if card.copy_count() == 0 {
                data.cards.remove(key);
            }
            Ok(before)
        })
    }

    /// Applies `change` to the cards with the given keys and saves.
    /// Applies `change` to the cards with the given keys and saves. If the change
    /// can't be made to any card (too many tags), nothing is saved.
    pub fn bulk_edit(&self, keys: &[String], change: &BulkChange) -> Result<BulkResult> {
        let mut change = change.clone();
        change.add_tags = normalize_tags(&change.add_tags)?;
        self.file.mutate(|data| {
            // Tags added take the spellings already in the collection.
            change.add_tags = with_spellings(std::mem::take(&mut change.add_tags), &tags_in_use(data.cards.values()));
            let mut res = BulkResult::default();
            let mut edited = Vec::new();
            for key in keys {
                let Some(c) = data.cards.get(key) else { continue };
                if change.delete {
                    edited.push((key.clone(), None));
                    continue;
                }
                let (card, skipped) = change.apply(c)?;
                res.skipped.extend(skipped);
                edited.push((key.clone(), Some(card)));
            }
            // Only once every card could be changed.
            for (key, card) in edited {
                match card {
                    Some(card) => {
                        data.cards.insert(key, card);
                        res.updated += 1;
                    }
                    None => {
                        data.cards.remove(&key);
                        res.deleted += 1;
                    }
                }
            }
            Ok(res)
        })
    }

    /// Every tag in use, sorted ignoring case.
    pub fn tag_names(&self) -> Vec<String> {
        self.file.read(|data| tags_in_use(data.cards.values()))
    }

    /// `tags` with each replaced by the spelling already in the collection when one
    /// matches ignoring case.
    pub fn canonical_tags(&self, tags: Vec<String>) -> Vec<String> {
        with_spellings(tags, &self.tag_names())
    }

    /// Renames a tag on every purchase that has it (after a backup). If `to` is
    /// already a tag, the two become one with `to`'s existing spelling, unless `from`
    /// and `to` differ only in case, when `to`'s new spelling is used everywhere.
    /// Purchases made identical merge.
    pub fn rename_tag(&self, from: &str, to: &str) -> Result<TagEdit> {
        let Some(to) = clean_tag(to) else { bail!("that tag can't be used: {:?}", to.trim()) };
        let target = if same_tag(from, &to) { to } else { self.tag_names().into_iter().find(|t| same_tag(t, &to)).unwrap_or(to) };
        self.edit_tag(from, "tag-rename", |tags| tags.push(target.clone()))
    }

    /// Removes a tag from every purchase that has it (after a backup).
    pub fn delete_tag(&self, name: &str) -> Result<TagEdit> {
        self.edit_tag(name, "tag-delete", |_| {})
    }

    /// Takes `name` off every purchase that has it, lets `then` change those
    /// purchases' tags, and merges purchases made identical. Backs up first when the
    /// tag is in use.
    fn edit_tag(&self, name: &str, reason: &str, then: impl Fn(&mut Vec<String>)) -> Result<TagEdit> {
        if !self.tag_names().iter().any(|t| same_tag(t, name)) {
            return Ok(TagEdit::default());
        }
        let backup = self.backup(reason)?;
        let changed = self.file.mutate(|data| {
            let mut changed = 0;
            for c in data.cards.values_mut() {
                let mut copies = super::Copies::default();
                for (f, cond, p) in c.copies.each() {
                    let mut p = p.clone();
                    if p.tags.iter().any(|t| same_tag(t, name)) {
                        p.tags.retain(|t| !same_tag(t, name));
                        then(&mut p.tags);
                        p.tags = normalize_tags(&p.tags)?;
                        changed += 1;
                    }
                    copies.add(f, cond, p);
                }
                c.copies = copies;
            }
            Ok(changed)
        })?;
        Ok(TagEdit { changed, backup: Some(backup) })
    }

    /// Whether any card's market prices were last refreshed before today.
    pub fn prices_stale(&self) -> bool {
        self.prices_older_than(1)
    }

    /// Whether any card's market prices are at least `days` old (1: from before today;
    /// 7: from a week ago or earlier).
    pub fn prices_older_than(&self, days: i64) -> bool {
        let cutoff = dates::format(dates::today() + chrono::Duration::days(1 - days));
        self.file.read(|d| d.cards.values().any(|c| !c.id.is_empty() && c.prices_updated < cutoff))
    }

    /// The distinct printing IDs in the collection.
    pub fn ids(&self) -> Vec<String> {
        self.file.read(|data| {
            let mut seen = HashSet::new();
            data.cards.values().filter(|c| !c.id.is_empty() && seen.insert(c.id.as_str())).map(|c| c.id.clone()).collect()
        })
    }

    /// Applies fresh prices (and missing rules text) from the game's catalog and saves. Returns the number of cards updated.
    pub fn update_catalog(&self, updates: &std::collections::HashMap<String, CatalogUpdate>, date: &str) -> Result<usize> {
        self.file.mutate(|data| {
            let mut updated = 0;
            for c in data.cards.values_mut() {
                let Some(u) = updates.get(&c.id) else { continue };
                c.market = u.market.clone();
                c.price_from = u.price_from.clone();
                c.prices_updated = date.to_string();
                if c.rules_text.is_empty() && !u.rules_text.is_empty() {
                    // Fill in text for cards saved without it.
                    c.type_line = u.type_line.clone();
                    c.rules_text = u.rules_text.clone();
                }
                updated += 1;
            }
            Ok(updated)
        })
    }

    /// The card with the given key.
    pub fn get(&self, key: &str) -> Option<Card> {
        self.file.read(|d| d.cards.get(key).cloned())
    }

    /// A copy of every card.
    pub fn cards(&self) -> Vec<Card> {
        self.cards_where(|_| true)
    }

    /// A copy of the cards `keep` accepts, without copying the rest.
    pub fn cards_where(&self, keep: impl Fn(&Card) -> bool) -> Vec<Card> {
        self.file.read(|d| d.cards.values().filter(|c| keep(c)).cloned().collect())
    }

    /// The number of cards (printings).
    pub fn len(&self) -> usize {
        self.file.read(|d| d.cards.len())
    }

    /// Whether the collection has no cards.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Parses a collection file written by this app (to import a backup or another
/// device's collection). Returns the file's game and its cards.
pub fn read_export(data: &[u8]) -> Result<(String, Vec<Card>)> {
    #[derive(Deserialize)]
    struct File {
        #[serde(default)]
        game: String,
        cards: Option<BTreeMap<String, Card>>,
    }
    let f: File = serde_json::from_slice(data).context("not a collection file")?;
    let cards = f.cards.ok_or_else(|| anyhow!("not a collection file: no \"cards\" found"))?;
    Ok((f.game, cards.into_values().filter(|c| !c.id.is_empty() && c.copy_count() > 0).collect()))
}

/// Applies the canonical spellings: lowercase finishes and language, and no empty
/// purchases.
fn normalize_card(c: &mut Card) {
    if c.language.is_empty() {
        c.language = "en".into();
    }
    c.language = c.language.to_lowercase();
    let mut copies = super::Copies::default();
    for (f, cond, p) in c.copies.each() {
        copies.add(f, cond, p.clone());
    }
    c.copies = copies;
}

/// Normalizes every purchase's tags (see `normalize_tags`).
fn normalize_card_tags(c: &mut Card) -> Result<()> {
    for by_cond in c.copies.0.values_mut() {
        for list in by_cond.values_mut() {
            for p in list.iter_mut() {
                p.tags = normalize_tags(&p.tags)?;
            }
        }
    }
    Ok(())
}

/// Gives `card`'s tags the spellings already in the collection, and one spelling
/// across its own purchases. Call `normalize_card` after, to merge purchases this
/// made identical.
fn use_existing_spellings(data: &StoreData, card: &mut Card) {
    let mut known = tags_in_use(data.cards.values());
    for t in tags_in_use(std::iter::once(&*card)) {
        if !known.iter().any(|k| same_tag(k, &t)) {
            known.push(t);
        }
    }
    for by_cond in card.copies.0.values_mut() {
        for list in by_cond.values_mut() {
            for p in list.iter_mut() {
                p.tags = with_spellings(std::mem::take(&mut p.tags), &known);
            }
        }
    }
}

/// Adds `incoming`'s purchases to an existing card for the same printing (identical
/// purchases combine). Catalog data (image, prices) comes from whichever side is newer;
/// fields only one side has, such as game attributes or rules text, are kept either way.
fn merge_cards(mut existing: Card, mut incoming: Card) -> Card {
    let mut copies = std::mem::take(&mut existing.copies);
    for (f, c, p) in std::mem::take(&mut incoming.copies).each() {
        copies.add(f, c, p.clone());
    }
    let (older, mut merged) = if incoming.prices_updated > existing.prices_updated { (existing, incoming) } else { (incoming, existing) };
    for (k, v) in older.attrs {
        merged.attrs.entry(k).or_insert(v);
    }
    if merged.rules_text.is_empty() {
        merged.type_line = older.type_line;
        merged.rules_text = older.rules_text;
    }
    merged.copies = copies;
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::{Condition, Purchase};

    fn card(id: &str, qty: i64) -> Card {
        let mut c = Card { id: id.into(), name: format!("Card {id}"), language: "EN".into(), ..Card::default() };
        c.add_purchase("nonfoil", Condition::NearMint, Purchase { quantity: qty, price_paid: 1.0, ..Default::default() });
        c.market.insert("nonfoil".into(), 2.0);
        c
    }

    #[test]
    fn add_save_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mtg/collection.json");
        let s = Store::open(&path, "mtg").unwrap();
        assert_eq!(s.add_cards(vec![card("a", 2), card("b", 1), card("", 5)]).unwrap(), (2, 0));
        assert_eq!(s.add_cards(vec![card("a", 3)]).unwrap(), (0, 1));
        assert_eq!(s.get("a").unwrap().copy_count(), 5);
        assert_eq!(s.get("a").unwrap().language, "en");

        let again = Store::open(&path, "mtg").unwrap();
        assert_eq!(again.len(), 2);
        assert!(Store::open(&path, "pokemon").is_err());
    }

    #[test]
    fn count_new_matches_add_cards() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(dir.path().join("c.json"), "mtg").unwrap();
        s.add_cards(vec![card("a", 1)]).unwrap();
        let incoming = vec![card("a", 1), card("b", 1), card("b", 2), card("c", 0)];
        let counted = s.count_new(&incoming);
        assert_eq!(counted, (1, 2));
        assert_eq!(s.add_cards(incoming).unwrap(), counted);
    }

    #[test]
    fn save_card_moves_and_deletes() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(dir.path().join("c.json"), "mtg").unwrap();
        s.add_cards(vec![card("a", 1), card("b", 2)]).unwrap();

        let mut moved = s.get("a").unwrap();
        moved.id = "b".into();
        assert_eq!(s.save_card("a", moved).unwrap(), ("b".into(), true));
        assert!(s.get("a").is_none());
        assert_eq!(s.get("b").unwrap().copy_count(), 3);

        let mut none = s.get("b").unwrap();
        none.copies = Default::default();
        s.save_card("b", none).unwrap();
        assert!(s.is_empty());

        let mut bad = card("c", 1);
        bad.copies.0.get_mut("nonfoil").unwrap().get_mut(&Condition::NearMint).unwrap()[0].quantity = -1;
        assert!(s.save_card("c", bad).is_err());
    }

    #[test]
    fn bulk_edit_and_catalog() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(dir.path().join("c.json"), "mtg").unwrap();
        s.add_cards(vec![card("a", 1), card("b", 2)]).unwrap();
        let change = BulkChange { condition: Some(Condition::HeavyPlay), ..Default::default() };
        let res = s.bulk_edit(&["a".into(), "missing".into()], &change).unwrap();
        assert_eq!((res.updated, res.deleted), (1, 0));
        assert_eq!(s.get("a").unwrap().purchases("nonfoil", Condition::HeavyPlay).len(), 1);
        assert_eq!(s.get("b").unwrap().purchases("nonfoil", Condition::NearMint).len(), 1);

        let mut updates = std::collections::HashMap::new();
        updates.insert(
            "b".to_string(),
            CatalogUpdate { market: [("nonfoil".to_string(), 9.0)].into(), rules_text: "Text".into(), ..Default::default() },
        );
        assert_eq!(s.update_catalog(&updates, "2024-01-01").unwrap(), 1);
        let b = s.get("b").unwrap();
        assert_eq!((b.market["nonfoil"], b.rules_text.as_str()), (9.0, "Text"));
        assert!(s.prices_stale());

        let res = s.bulk_edit(&["a".into()], &BulkChange { delete: true, ..Default::default() }).unwrap();
        assert_eq!(res.deleted, 1);
        assert_eq!(s.ids(), ["b"]);
    }

    #[test]
    fn two_stores_on_one_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("collection.json");
        let here = Store::open(&path, "mtg").unwrap();
        let there = Store::open(&path, "mtg").unwrap();
        here.add_cards(vec![card("a", 1)]).unwrap();
        there.add_cards(vec![card("b", 2)]).unwrap();
        here.add_cards(vec![card("c", 3)]).unwrap();
        there.refresh().unwrap();
        for s in [&here, &there] {
            assert_eq!(s.ids(), ["a", "b", "c"]);
        }
    }

    #[test]
    fn half_synced_file_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("collection.json");
        let s = Store::open(&path, "mtg").unwrap();
        s.add_cards(vec![card("a", 1), card("b", 1)]).unwrap();
        let whole = fs::read(&path).unwrap();
        fs::write(&path, &whole[..whole.len() / 2]).unwrap();
        let err = s.add_cards(vec![card("c", 1)]).unwrap_err();
        assert!(err.to_string().contains("being updated by another device"), "{err}");
        assert_eq!(fs::read(&path).unwrap(), &whole[..whole.len() / 2]);
        assert_eq!(s.len(), 2);
    }

    #[test]
    fn touched_file_is_not_reloaded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("collection.json");
        let s = Store::open(&path, "mtg").unwrap();
        s.add_cards(vec![card("a", 1)]).unwrap();
        let rev = s.revision();
        let touch = |t| fs::File::options().write(true).open(&path).unwrap().set_modified(t).unwrap();
        touch(std::time::SystemTime::now() - std::time::Duration::from_secs(60));
        assert!(!s.refresh().unwrap());
        touch(std::time::SystemTime::now());
        assert!(!s.refresh().unwrap());
        assert_eq!(s.revision(), rev);
    }

    #[test]
    fn backups_keep_ten_and_restore() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(dir.path().join("mtg/collection.json"), "mtg").unwrap();
        s.add_cards(vec![card("a", 1)]).unwrap();
        let first = s.backup("import").unwrap();
        s.add_cards(vec![card("b", 1)]).unwrap();
        for _ in 0..12 {
            s.backup("bulk-edit").unwrap(); // more than are kept, in one second, never overwriting
        }
        let list = s.backups().unwrap();
        assert_eq!(list.len(), BACKUPS_KEPT);
        assert!(list.iter().all(|b| b.name != first), "the oldest went");
        assert_eq!((list[0].reason.as_str(), list[0].cards), ("bulk-edit", 2));
        assert!(list[0].size > 0 && list[0].date.len() == 19);

        let kept = s.backups().unwrap().last().unwrap().name.clone();
        s.bulk_edit(&["a".into(), "b".into()], &BulkChange { delete: true, ..Default::default() }).unwrap();
        let taken = s.restore(&kept).unwrap();
        assert_eq!(s.ids(), ["a", "b"]);
        let after = s.backups().unwrap();
        assert_eq!((after[0].name.as_str(), after[0].reason.as_str(), after[0].cards), (taken.as_str(), "restore", 0));

        for bad in ["../collection.json", "collection.json", "nope", &format!("../backups/{kept}")] {
            assert!(s.restore(bad).is_err(), "{bad}");
        }
        assert!(s.backup("whatever").is_err());
    }

    #[test]
    fn merging_a_conflict_copy_counts_nothing_twice() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(dir.path().join("c.json"), "mtg").unwrap();
        s.add_cards(vec![card("a", 2), card("b", 1)]).unwrap();
        // The other device: a with 3 of the same purchase, a second purchase of a, no b, and c.
        let mut a = card("a", 3);
        a.add_purchase("foil", Condition::LightPlay, Purchase { quantity: 1, price_paid: 5.0, ..Default::default() });
        assert_eq!(s.merge_copy(vec![a, card("c", 1)]).unwrap(), 2);
        let a = s.get("a").unwrap();
        assert_eq!((a.copy_count(), a.purchases("nonfoil", Condition::NearMint)[0].quantity), (4, 3));
        assert_eq!(s.get("b").unwrap().copy_count(), 1, "kept though the copy lacks it");
        assert_eq!(s.get("c").unwrap().copy_count(), 1);
        // Merging the same copy again changes nothing.
        let again = s.cards();
        assert_eq!(s.merge_copy(again).unwrap(), 0);
    }

    #[test]
    fn lots_reprice_and_drop() {
        use crate::lots::{Lot, LotCount, LotSplit, lot_prices};
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(dir.path().join("c.json"), "mtg").unwrap();
        let in_lot = |id: &str, lot: &str, q: i64| {
            let mut c = card(id, 0);
            c.add_purchase("nonfoil", Condition::NearMint, Purchase { quantity: q, lot: lot.into(), ..Default::default() });
            c
        };
        s.add_cards(vec![in_lot("a", "L1", 2), in_lot("b", "L1", 1), in_lot("a", "L2", 1), card("c", 1)]).unwrap();
        assert_eq!(s.get("a").unwrap().purchases("nonfoil", Condition::NearMint).len(), 2, "copies from two lots don't merge");

        let lot = Lot {
            id: "L1".into(),
            name: "x".into(),
            total_paid: 10.0,
            split: LotSplit::ByValue,
            count: LotCount::AsEntered,
            ..Lot::default()
        };
        let copies = s.lot_copies("L1", |_| false);
        assert_eq!(copies.iter().map(|c| c.quantity).sum::<i64>(), 3);
        let prices = lot_prices(&lot, &copies);
        assert_eq!(s.reprice_lot("L1", &prices, "2026-11-14").unwrap(), 2);
        let paid: f64 = s
            .lot_cards("L1")
            .iter()
            .flat_map(|c| c.purchase_rows())
            .filter(|r| r.purchase.lot == "L1")
            .map(|r| r.purchase.price_paid * r.purchase.quantity as f64)
            .sum();
        assert!((paid - 10.0).abs() < 1e-9, "{paid}");
        assert!(
            s.get("a").unwrap().purchase_rows().iter().filter(|r| r.purchase.lot == "L1").all(|r| r.purchase.acquired_date == "2026-11-14")
        );

        // Keep: prices stay, the lot goes; remove: the copies go.
        s.drop_lot("L1", false).unwrap();
        assert!(s.lot_cards("L1").is_empty());
        assert_eq!(s.get("b").unwrap().copy_count(), 1);
        s.drop_lot("L2", true).unwrap();
        assert_eq!(s.get("a").unwrap().copy_count(), 2);
        assert_eq!(s.len(), 3);
    }

    #[test]
    fn backup_names() {
        assert_eq!(parse_backup_name("collection-20261114-093005-import.json"), Some(("2026-11-14 09:30:05".into(), 1, "import".into())));
        assert_eq!(
            parse_backup_name("collection-20261114-093005-12-bulk-delete.json").unwrap(),
            ("2026-11-14 09:30:05".into(), 12, "bulk-delete".into())
        );
        assert_eq!(parse_backup_name("collection-20261114-093005-bulk-delete.json").unwrap().1, 1);
        assert_eq!(parse_backup_name("collection-20261114-093005-other.json"), None);
        assert_eq!(parse_backup_name("notes.txt"), None);
    }

    #[test]
    fn export_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(dir.path().join("c.json"), "mtg").unwrap();
        s.add_cards(vec![card("a", 1)]).unwrap();
        let (game, cards) = read_export(&fs::read(s.file_path()).unwrap()).unwrap();
        assert_eq!((game.as_str(), cards.len()), ("mtg", 1));
        assert!(read_export(b"{}").is_err());
        assert!(read_export(b"name,qty").is_err());
    }

    fn tagged(id: &str, qty: i64, tags: &[&str]) -> Card {
        let mut c = card(id, 0);
        c.add_purchase(
            "nonfoil",
            Condition::NearMint,
            Purchase { quantity: qty, price_paid: 1.0, tags: tags.iter().map(|t| t.to_string()).collect(), ..Default::default() },
        );
        c
    }

    fn tags_of(s: &Store, key: &str) -> Vec<(i64, Vec<String>)> {
        let mut out: Vec<(i64, Vec<String>)> =
            s.get(key).unwrap().purchase_rows().into_iter().map(|r| (r.purchase.quantity, r.purchase.tags)).collect();
        out.sort();
        out
    }

    fn v(t: &[&str]) -> Vec<String> {
        t.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn tags_keep_one_spelling() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(dir.path().join("c.json"), "mtg").unwrap();
        s.add_cards(vec![tagged("a", 1, &["Binder A"])]).unwrap();
        s.add_cards(vec![tagged("b", 1, &["  binder   a", "Deck"]), tagged("c", 1, &["deck"])]).unwrap();
        assert_eq!(tags_of(&s, "b"), [(1, v(&["Binder A", "Deck"]))], "the collection's spelling; cleaned");
        assert_eq!(tags_of(&s, "c"), [(1, v(&["Deck"]))], "one spelling within one save too");
        assert_eq!(s.tag_names(), ["Binder A", "Deck"]);
        assert_eq!(s.canonical_tags(v(&["DECK", "new"])), ["Deck", "new"]);
        assert!(s.add_cards(vec![tagged("d", 1, &["a,b"])]).is_err(), "invalid tags are refused");

        // Saving a card takes the rest of the collection's spelling...
        let mut c = s.get("c").unwrap();
        c.copies.0.get_mut("nonfoil").unwrap().get_mut(&Condition::NearMint).unwrap()[0].tags = v(&["binder a"]);
        s.save_card("c", c).unwrap();
        assert_eq!(tags_of(&s, "c"), [(1, v(&["Binder A"]))]);
        // ...but a tag only this card has keeps the spelling it's saved with.
        let mut b = s.get("b").unwrap();
        b.copies.0.get_mut("nonfoil").unwrap().get_mut(&Condition::NearMint).unwrap()[0].tags = v(&["Binder A", "DECK"]);
        s.save_card("b", b).unwrap();
        assert_eq!(tags_of(&s, "b"), [(1, v(&["Binder A", "DECK"]))]);
        // Purchases made identical by a spelling merge.
        s.add_cards(vec![tagged("b", 2, &["binder a", "deck"])]).unwrap();
        assert_eq!(tags_of(&s, "b"), [(3, v(&["Binder A", "DECK"]))]);
    }

    #[test]
    fn bulk_tags_fail_whole_or_apply() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(dir.path().join("c.json"), "mtg").unwrap();
        let nineteen: Vec<String> = (0..19).map(|i| format!("t{i:02}")).collect();
        let full = tagged("full", 1, &nineteen.iter().map(String::as_str).collect::<Vec<_>>());
        s.add_cards(vec![tagged("a", 1, &["Deck"]), full]).unwrap();
        let keys = v(&["a", "full"]);
        let two = BulkChange { add_tags: v(&["x", "y"]), ..Default::default() };
        assert!(s.bulk_edit(&keys, &two).is_err());
        assert_eq!(tags_of(&s, "a"), [(1, v(&["Deck"]))], "nothing saved");
        let one = BulkChange { add_tags: v(&["deck", "x"]), remove_tags: v(&["t00"]), ..Default::default() };
        assert_eq!(s.bulk_edit(&keys, &one).unwrap().updated, 2);
        assert_eq!(tags_of(&s, "a"), [(1, v(&["Deck", "x"]))], "added in the collection's spelling");
        assert!(s.bulk_edit(&keys, &BulkChange { add_tags: v(&["a;b"]), ..Default::default() }).is_err());
    }

    #[test]
    fn rename_and_delete_tags() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(dir.path().join("c.json"), "mtg").unwrap();
        s.add_cards(vec![tagged("a", 1, &["elves"]), tagged("a", 2, &["Binder A"]), tagged("b", 1, &["elves", "keep"])]).unwrap();
        assert_eq!(s.rename_tag("nope", "x").unwrap(), TagEdit::default(), "not in use: nothing, no backup");
        assert!(s.rename_tag("elves", "a;b").is_err());

        let r = s.rename_tag("ELVES", "elf deck").unwrap();
        assert_eq!(r.changed, 2);
        assert!(r.backup.unwrap().ends_with("-tag-rename.json"));
        assert_eq!(s.tag_names(), ["Binder A", "elf deck", "keep"]);

        // Into an existing tag: one tag, the existing spelling; purchases made identical merge.
        assert_eq!(s.rename_tag("elf deck", "binder a").unwrap().changed, 2);
        assert_eq!(tags_of(&s, "a"), [(3, v(&["Binder A"]))]);
        assert_eq!(tags_of(&s, "b"), [(1, v(&["Binder A", "keep"]))]);

        // Case only: the new spelling everywhere.
        s.rename_tag("binder a", "BINDER A").unwrap();
        assert_eq!(s.tag_names(), ["BINDER A", "keep"]);

        let d = s.delete_tag("keep").unwrap();
        assert_eq!(d.changed, 1);
        assert!(d.backup.unwrap().ends_with("-tag-delete.json"));
        assert_eq!(s.tag_names(), ["BINDER A"]);
    }

    #[test]
    fn tags_in_files_merges_lots_and_sales() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("c.json");
        // An old file with no tags loads, and an untagged purchase writes no tags key.
        std::fs::write(&path, r#"{"game":"mtg","cards":{"a":{"id":"a","copies":{"nonfoil":{"NM":[{"quantity":1}]}}}}}"#).unwrap();
        let s = Store::open(&path, "mtg").unwrap();
        assert!(s.get("a").unwrap().purchase_rows()[0].purchase.tags.is_empty());
        s.add_cards(vec![tagged("b", 1, &[])]).unwrap();
        assert!(!std::fs::read_to_string(&path).unwrap().contains("tags"));

        // A conflict copy: purchases with the same tags count once; different tags are kept.
        s.add_cards(vec![tagged("c", 2, &["deck"])]).unwrap();
        s.merge_copy(vec![tagged("c", 1, &["Deck"]), tagged("c", 1, &["binder"])]).unwrap();
        assert_eq!(tags_of(&s, "c"), [(1, v(&["binder"])), (2, v(&["deck"]))]);

        // Taking copies out (a sale) needs the tags to match.
        let p = Purchase { price_paid: 1.0, tags: v(&["deck"]), ..Default::default() };
        assert!(s.take_copies("c", "nonfoil", Condition::NearMint, &Purchase { tags: v(&["other"]), ..p.clone() }, 1).is_err());
        assert_eq!(s.take_copies("c", "nonfoil", Condition::NearMint, &p, 1).unwrap().tags, ["deck"]);

        // Repricing a lot keeps tags.
        let mut in_lot = card("d", 0);
        in_lot.add_purchase(
            "nonfoil",
            Condition::NearMint,
            Purchase { quantity: 2, lot: "L".into(), tags: v(&["deck"]), ..Default::default() },
        );
        s.add_cards(vec![in_lot]).unwrap();
        let prices = crate::lots::LotPrices {
            each: [(("d".to_string(), "nonfoil".to_string(), Condition::NearMint), 1.5)].into(),
            ..Default::default()
        };
        s.reprice_lot("L", &prices, "2026-11-15").unwrap();
        let p = &s.get("d").unwrap().purchase_rows()[0].purchase;
        assert_eq!((p.price_paid, p.tags.clone()), (1.5, v(&["deck"])));
    }

    #[test]
    fn moving_copies_into_and_out_of_lots() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(dir.path().join("c.json"), "mtg").unwrap();
        // a: 3 copies at $1 (no lot) and 2 in lot "old"; b: 1 copy at $1.
        let mut a = card("a", 3);
        a.add_purchase("nonfoil", Condition::NearMint, Purchase { quantity: 2, price_paid: 0.5, lot: "old".into(), ..Default::default() });
        s.add_cards(vec![a, card("b", 1)]).unwrap();
        let pick = |key: &str, q: Option<i64>| LotPick { key: key.into(), quantity: q };
        let in_lot = |key: &str, lot: &str| -> i64 {
            s.get(key).map_or(0, |c| c.copies.each().filter(|(_, _, p)| p.lot == lot).map(|(_, _, p)| p.quantity).sum())
        };

        // Two copies of a: taken from the copies in no lot first.
        assert_eq!(s.put_in_lot("new", "2026-11-15", &[pick("a", Some(2))]).unwrap(), (2, vec![]));
        assert_eq!((in_lot("a", "new"), in_lot("a", ""), in_lot("a", "old")), (2, 1, 2));
        let joined = s.get("a").unwrap().purchase_rows().into_iter().find(|r| r.purchase.lot == "new").unwrap().purchase;
        assert_eq!(joined.acquired_date, "2026-11-15", "the lot's date");
        // Every other copy of a (1 in no lot, 2 from "old"), and all of b.
        let (moved, left) = s.put_in_lot("new", "2026-11-15", &[pick("a", None), pick("b", None)]).unwrap();
        assert_eq!((moved, left), (4, vec!["old".to_string()]));
        assert_eq!((in_lot("a", "new"), in_lot("a", "old"), in_lot("b", "new")), (5, 0, 1));
        assert_eq!(s.get("a").unwrap().copy_count(), 5, "no copies made or lost");
        assert!(s.put_in_lot("new", "", &[pick("a", None)]).is_err(), "nothing left to join");
        assert!(s.put_in_lot("new", "", &[pick("b", Some(2))]).is_err());
        assert!(s.put_in_lot("new", "", &[pick("nope", None)]).is_err());

        // Out of the lot: the copies stay, at the price the lot gave them.
        let mut priced = s.get("b").unwrap();
        priced.copies.0.get_mut("nonfoil").unwrap().get_mut(&Condition::NearMint).unwrap()[0].price_paid = 0.37;
        s.save_card("b", priced).unwrap();
        assert_eq!(s.remove_from_lot("new", "b").unwrap(), 1);
        let p = s.get("b").unwrap().purchase_rows()[0].purchase.clone();
        assert_eq!((p.lot.as_str(), p.price_paid, p.quantity), ("", 0.37, 1));
        assert!(s.remove_from_lot("new", "b").is_err(), "not in the lot any more");
    }
}
