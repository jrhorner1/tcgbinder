use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use super::{BulkChange, BulkResult, Card, CatalogUpdate};
use crate::dates;
use crate::jsonfile::JsonFile;

/// One game's collection, persisted as a JSON file. Each game has its own. The file
/// may also be changed by another device through a synced folder: every change first
/// takes in what is on disk (see [`JsonFile`]).
pub struct Store {
    file: JsonFile<StoreData>,
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
pub const BACKUP_REASONS: &[&str] = &["import", "bulk-edit", "bulk-delete", "restore", "merge", "move"];

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
    pub fn add_cards(&self, cards: Vec<Card>) -> Result<(usize, usize)> {
        self.file.mutate(|data| {
            let (mut created, mut merged) = (0, 0);
            for mut card in cards {
                if card.id.is_empty() || card.copy_count() <= 0 {
                    continue;
                }
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
        normalize_card(&mut card);
        let new_key = card_key(&card);
        self.file.mutate(|data| {
            let mut merged = false;
            data.cards.remove(old_key);
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

    /// Applies `change` to the cards with the given keys and saves.
    pub fn bulk_edit(&self, keys: &[String], change: &BulkChange) -> Result<BulkResult> {
        self.file.mutate(|data| {
            let mut res = BulkResult::default();
            for key in keys {
                let Some(c) = data.cards.get(key) else { continue };
                if change.delete {
                    data.cards.remove(key);
                    res.deleted += 1;
                    continue;
                }
                let (edited, skipped) = change.apply(c);
                res.skipped.extend(skipped);
                data.cards.insert(key.clone(), edited);
                res.updated += 1;
            }
            Ok(res)
        })
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
}
