//! The collection file: one game's cards and the copies owned, opened, read and
//! changed here. Backups, tags (and lots, which are tags) and bulk changes are in the
//! submodules, each an `impl Store` block.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use super::tags::{normalize_tags, same_tag, same_tags, tags_in_use, with_spellings};
// `Copies` is here for the submodules, whose code names it as `super::Copies`.
use super::{Card, CatalogUpdate, Condition, Copies, Purchase};
use crate::dates;
use crate::jsonfile::JsonFile;

mod backups;
mod bulk;
mod lot_list;
mod tagging;

pub use lot_list::{ListFound, ListLine, ListMatch};
pub use tagging::{CopyPick, LostLot, TagEdit};

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

    /// Deletes every card (deleting all data). Back up first if it may be wanted.
    pub fn clear(&self) -> Result<()> {
        self.file.mutate(|d| {
            d.cards.clear();
            Ok(())
        })
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
    /// one in `finish` and `cond` with `purchase`'s price paid, acquired date and tags.
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
    use std::fs;

    pub(super) fn card(id: &str, qty: i64) -> Card {
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
    fn export_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(dir.path().join("c.json"), "mtg").unwrap();
        s.add_cards(vec![card("a", 1)]).unwrap();
        let (game, cards) = read_export(&fs::read(s.file_path()).unwrap()).unwrap();
        assert_eq!((game.as_str(), cards.len()), ("mtg", 1));
        assert!(read_export(b"{}").is_err());
        assert!(read_export(b"name,qty").is_err());
    }

    pub(super) fn tagged(id: &str, qty: i64, tags: &[&str]) -> Card {
        let mut c = card(id, 0);
        c.add_purchase(
            "nonfoil",
            Condition::NearMint,
            Purchase { quantity: qty, price_paid: 1.0, tags: tags.iter().map(|t| t.to_string()).collect(), ..Default::default() },
        );
        c
    }

    pub(super) fn tags_of(s: &Store, key: &str) -> Vec<(i64, Vec<String>)> {
        let mut out: Vec<(i64, Vec<String>)> =
            s.get(key).unwrap().purchase_rows().into_iter().map(|r| (r.purchase.quantity, r.purchase.tags)).collect();
        out.sort();
        out
    }

    pub(super) fn v(t: &[&str]) -> Vec<String> {
        t.iter().map(|s| s.to_string()).collect()
    }
}
