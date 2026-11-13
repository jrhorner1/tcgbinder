use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, RwLock};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use super::{BulkChange, BulkResult, Card, CatalogUpdate};
use crate::dates;

/// One game's collection, persisted as a JSON file. Each game has its own.
pub struct Store {
    path: PathBuf,
    data: RwLock<StoreData>,
    /// Serializes saves so an older state never lands last.
    save_lock: Mutex<()>,
    /// Counts changes, so front ends can tell when to show fresh data.
    revision: AtomicU64,
}

/// The collection file's contents.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct StoreData {
    /// The ID of the game this file belongs to.
    #[serde(skip_serializing_if = "String::is_empty")]
    game: String,
    cards: BTreeMap<String, Card>,
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
        let path = std::path::absolute(path.as_ref()).context("resolving the collection path")?;
        let data = match fs::read(&path) {
            Ok(b) if b.iter().all(u8::is_ascii_whitespace) => StoreData::default(),
            Ok(b) => serde_json::from_slice(&b).with_context(|| format!("malformed collection file {}", path.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => StoreData::default(),
            Err(e) => return Err(e).context("failed to load existing collection"),
        };
        let owner = data.game.clone();
        let store = Store { path, data: RwLock::new(data), save_lock: Mutex::new(()), revision: AtomicU64::new(0) };
        if owner.is_empty() {
            store.data.write().unwrap().game = game.to_string();
            store.save()?;
        } else if owner != game {
            bail!("{} holds a {owner:?} collection, not {game:?}", store.path.display());
        }
        Ok(store)
    }

    /// The absolute path of the collection file.
    pub fn file_path(&self) -> &Path {
        &self.path
    }

    /// A number that changes whenever the collection does.
    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::Relaxed)
    }

    /// Writes the collection to disk atomically: a temporary file is written, synced and
    /// renamed over the old one, so a crash mid-save never corrupts it.
    pub fn save(&self) -> Result<()> {
        let _serial = self.save_lock.lock().unwrap();
        let dir = self.path.parent().ok_or_else(|| anyhow!("no folder for {}", self.path.display()))?;
        fs::create_dir_all(dir).context("failed to create storage directories")?;
        let tmp = tempfile::Builder::new().prefix("collection_").suffix(".tmp").tempfile_in(dir)?;
        {
            // Indented for human-readable diffs; buffered, as the encoder makes many small writes.
            let mut w = BufWriter::with_capacity(1 << 16, tmp.as_file());
            let data = self.data.read().unwrap();
            serde_json::to_writer_pretty(&mut w, &*data).context("failed to encode collection JSON")?;
            w.write_all(b"\n")?;
            w.flush()?;
        }
        tmp.as_file().sync_all().context("failed to sync the collection file")?;
        tmp.persist(&self.path).context("failed to replace the collection file")?;
        self.revision.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Adds copies to the collection, merging into the card for the same printing when
    /// there is one, and saves once. It returns how many new cards were created and how
    /// many cards were added to existing ones.
    pub fn add_cards(&self, cards: Vec<Card>) -> Result<(usize, usize)> {
        let (mut created, mut merged) = (0, 0);
        {
            let mut data = self.data.write().unwrap();
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
        }
        self.save()?;
        Ok((created, merged))
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
        let mut merged = false;
        {
            let mut data = self.data.write().unwrap();
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
        }
        self.save()?;
        Ok((new_key, merged))
    }

    /// Drops the card with the given key (doing nothing if there is none).
    pub fn remove_card(&self, key: &str) -> Result<()> {
        {
            let mut data = self.data.write().unwrap();
            if data.cards.remove(key).is_none() {
                return Ok(());
            }
        }
        self.save()
    }

    /// Applies `change` to the cards with the given keys and saves.
    pub fn bulk_edit(&self, keys: &[String], change: &BulkChange) -> Result<BulkResult> {
        let mut res = BulkResult::default();
        {
            let mut data = self.data.write().unwrap();
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
        }
        self.save()?;
        Ok(res)
    }

    /// Whether any card's market prices were last refreshed before today.
    pub fn prices_stale(&self) -> bool {
        self.prices_older_than(1)
    }

    /// Whether any card's market prices are at least `days` old (1: from before today;
    /// 7: from a week ago or earlier).
    pub fn prices_older_than(&self, days: i64) -> bool {
        let cutoff = dates::format(dates::today() + chrono::Duration::days(1 - days));
        self.data.read().unwrap().cards.values().any(|c| !c.id.is_empty() && c.prices_updated < cutoff)
    }

    /// The distinct printing IDs in the collection.
    pub fn ids(&self) -> Vec<String> {
        let data = self.data.read().unwrap();
        let mut seen = HashSet::new();
        data.cards.values().filter(|c| !c.id.is_empty() && seen.insert(c.id.as_str())).map(|c| c.id.clone()).collect()
    }

    /// Applies fresh prices (and missing rules text) from the game's catalog and saves. Returns the number of cards updated.
    pub fn update_catalog(&self, updates: &std::collections::HashMap<String, CatalogUpdate>, date: &str) -> Result<usize> {
        let mut updated = 0;
        {
            let mut data = self.data.write().unwrap();
            for c in data.cards.values_mut() {
                let Some(u) = updates.get(&c.id) else { continue };
                c.market = u.market.clone();
                c.prices_updated = date.to_string();
                if c.rules_text.is_empty() && !u.rules_text.is_empty() {
                    // Fill in text for cards saved without it.
                    c.type_line = u.type_line.clone();
                    c.rules_text = u.rules_text.clone();
                }
                updated += 1;
            }
        }
        self.save()?;
        Ok(updated)
    }

    /// The card with the given key.
    pub fn get(&self, key: &str) -> Option<Card> {
        self.data.read().unwrap().cards.get(key).cloned()
    }

    /// A copy of every card.
    pub fn cards(&self) -> Vec<Card> {
        self.cards_where(|_| true)
    }

    /// A copy of the cards `keep` accepts, without copying the rest.
    pub fn cards_where(&self, keep: impl Fn(&Card) -> bool) -> Vec<Card> {
        self.data.read().unwrap().cards.values().filter(|c| keep(c)).cloned().collect()
    }

    /// The number of cards (printings).
    pub fn len(&self) -> usize {
        self.data.read().unwrap().cards.len()
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
