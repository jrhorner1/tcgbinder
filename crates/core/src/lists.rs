//! Lists and decks: cards wanted or played together, each entry a catalog copy of the
//! card (no copies held) with how many are wanted. Kept in `lists.json` beside the
//! collection. An entry counts as owned from the collection: copies of that printing,
//! or with `any_printing`, of every printing with the card's name.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::collection::{Card, CatalogUpdate, Copies};
use crate::jsonfile::JsonFile;
use crate::lots::new_id;

/// The lists file in each game's data folder.
pub const LISTS_FILE: &str = "lists.json";

fn is_false(b: &bool) -> bool {
    !*b
}

/// A card on a list.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ListEntry {
    /// A catalog copy of the card (no copies), with market prices.
    pub card: Card,
    pub quantity: i64,
    /// Any printing with this card's name counts as owned.
    #[serde(skip_serializing_if = "is_false")]
    pub any_printing: bool,
    /// The finish wanted; "" for any.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub finish: String,
}

impl ListEntry {
    /// An entry for `card`, dropping any copies it carries.
    pub fn new(mut card: Card, quantity: i64, any_printing: bool, finish: &str) -> ListEntry {
        card.copies = Copies::default();
        ListEntry { card, quantity, any_printing, finish: finish.trim().to_lowercase() }
    }

    /// Whether two entries are for the same thing (and so combine).
    fn same_as(&self, other: &ListEntry) -> bool {
        self.card.id == other.card.id && self.any_printing == other.any_printing && self.finish == other.finish
    }

    /// The market price of one copy: in the finish wanted, else the first finish in
    /// `finish_order` with a price, else the lowest price there is.
    pub fn price(&self, finish_order: &[&str]) -> Option<f64> {
        if !self.finish.is_empty() {
            return self.card.market.get(&self.finish).copied();
        }
        finish_order.iter().find_map(|f| self.card.market.get(*f).copied()).or_else(|| self.card.market.values().copied().reduce(f64::min))
    }

    /// The copies owned that count toward this entry.
    pub fn owned(&self, collection: &[Card]) -> i64 {
        let counts = |c: &Card| if self.finish.is_empty() { c.copy_count() } else { c.finish_copy_count(&self.finish) };
        if self.any_printing {
            let name = self.card.name.to_lowercase();
            collection.iter().filter(|c| c.name.to_lowercase() == name).map(counts).sum()
        } else {
            collection.iter().filter(|c| c.id == self.card.id).map(counts).sum()
        }
    }
}

/// A list or deck.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct List {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub notes: String,
    pub created: String,
    pub entries: Vec<ListEntry>,
}

impl List {
    /// Checks the name and quantities, and combines entries for the same thing.
    pub fn validate(&mut self) -> Result<()> {
        self.name = self.name.trim().to_string();
        if self.name.is_empty() {
            bail!("give the list a name");
        }
        if self.entries.iter().any(|e| e.card.id.is_empty()) {
            bail!("every entry needs a card");
        }
        if self.entries.iter().any(|e| e.quantity <= 0) {
            bail!("quantities must be at least 1");
        }
        let mut combined: Vec<ListEntry> = Vec::new();
        for e in self.entries.drain(..) {
            match combined.iter_mut().find(|c| c.same_as(&e)) {
                Some(c) => c.quantity += e.quantity,
                None => combined.push(ListEntry { card: Card { copies: Copies::default(), ..e.card }, ..e }),
            }
        }
        self.entries = combined;
        Ok(())
    }

    /// Adds an entry, or grows the one for the same thing.
    pub fn add(&mut self, entry: ListEntry) {
        match self.entries.iter_mut().find(|c| c.same_as(&entry)) {
            Some(c) => c.quantity += entry.quantity,
            None => self.entries.push(entry),
        }
    }
}

/// The lists file's contents.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ListsData {
    pub lists: Vec<List>,
}

/// One game's lists, kept in `lists.json` (changed by other devices too, through
/// [`JsonFile`]).
pub struct ListsStore {
    file: JsonFile<ListsData>,
}

impl ListsStore {
    /// Opens (or starts) the lists file in a game's data folder.
    pub fn open(game_dir: &Path) -> Result<ListsStore> {
        Ok(ListsStore { file: JsonFile::open(game_dir.join(LISTS_FILE), true)? })
    }

    /// Every list, by name.
    pub fn all(&self) -> Vec<List> {
        let mut lists = self.file.read(|d| d.lists.clone());
        lists.sort_by_key(|l| l.name.to_lowercase());
        lists
    }

    pub fn get(&self, id: &str) -> Option<List> {
        self.file.read(|d| d.lists.iter().find(|l| l.id == id).cloned())
    }

    /// Adds a new list (giving it an ID and creation time) and returns it.
    pub fn create(&self, mut list: List) -> Result<List> {
        list.validate()?;
        list.id = new_id();
        list.created = chrono::Local::now().to_rfc3339();
        self.file.mutate(|d| {
            d.lists.push(list.clone());
            Ok(())
        })?;
        Ok(list)
    }

    /// Saves a changed list (matched by ID), returning it as saved.
    pub fn save(&self, mut list: List) -> Result<List> {
        list.validate()?;
        self.file.mutate(|d| match d.lists.iter_mut().find(|l| l.id == list.id) {
            Some(l) => {
                *l = list.clone();
                Ok(())
            }
            None => bail!("there is no list {:?}", list.id),
        })?;
        Ok(list)
    }

    /// Adds entries to a list, returning it as saved.
    pub fn add_entries(&self, id: &str, entries: Vec<ListEntry>) -> Result<List> {
        self.file.mutate(|d| {
            let Some(l) = d.lists.iter_mut().find(|l| l.id == id) else { bail!("there is no list {id:?}") };
            for e in entries {
                if e.quantity <= 0 || e.card.id.is_empty() {
                    bail!("an entry needs a card and a quantity of at least 1");
                }
                l.add(ListEntry::new(e.card, e.quantity, e.any_printing, &e.finish));
            }
            Ok(l.clone())
        })
    }

    /// Deletes a list.
    pub fn remove(&self, id: &str) -> Result<()> {
        self.file.mutate(|d| {
            d.lists.retain(|l| l.id != id);
            Ok(())
        })
    }

    /// The catalog IDs on every list, for the price refresh.
    pub fn ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.file.read(|d| d.lists.iter().flat_map(|l| l.entries.iter().map(|e| e.card.id.clone())).collect());
        ids.sort();
        ids.dedup();
        ids
    }

    /// Applies a price refresh to the lists' cards. Returns how many entries changed.
    pub fn update_catalog(&self, updates: &HashMap<String, CatalogUpdate>, date: &str) -> Result<usize> {
        if self.file.read(|d| d.lists.iter().all(|l| l.entries.iter().all(|e| !updates.contains_key(&e.card.id)))) {
            return Ok(0);
        }
        self.file.mutate(|d| {
            let mut n = 0;
            for e in d.lists.iter_mut().flat_map(|l| l.entries.iter_mut()) {
                let Some(u) = updates.get(&e.card.id) else { continue };
                e.card.market = u.market.clone();
                e.card.price_from = u.price_from.clone();
                e.card.prices_updated = date.to_string();
                n += 1;
            }
            Ok(n)
        })
    }

    /// Merges a conflict copy of the lists file: lists by ID; within a list both
    /// sides' entries, the larger quantity where both have one. Returns how many
    /// lists changed.
    pub fn merge_copy(&self, theirs: ListsData) -> Result<usize> {
        self.file.mutate(|d| {
            let mut changed = 0;
            for list in theirs.lists {
                match d.lists.iter_mut().find(|l| l.id == list.id) {
                    None => {
                        d.lists.push(list);
                        changed += 1;
                    }
                    Some(ours) => {
                        let before = ours.entries.clone();
                        for e in list.entries {
                            match ours.entries.iter_mut().find(|o| o.same_as(&e)) {
                                Some(o) => o.quantity = o.quantity.max(e.quantity),
                                None => ours.entries.push(e),
                            }
                        }
                        if ours.entries != before {
                            changed += 1;
                        }
                    }
                }
            }
            Ok(changed)
        })
    }

    /// Takes in changes another device made.
    pub fn refresh(&self) -> Result<bool> {
        self.file.refresh()
    }

    pub fn path(&self) -> &Path {
        self.file.path()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::{Condition, Purchase};

    fn card(id: &str, name: &str, prices: &[(&str, f64)]) -> Card {
        Card { id: id.into(), name: name.into(), market: prices.iter().map(|(f, p)| (f.to_string(), *p)).collect(), ..Card::default() }
    }

    fn held(id: &str, name: &str, finish: &str, q: i64) -> Card {
        let mut c = card(id, name, &[]);
        c.add_purchase(finish, Condition::NearMint, Purchase { quantity: q, ..Default::default() });
        c
    }

    #[test]
    fn owned_and_price() {
        let collection = [held("bolt-m11", "Lightning Bolt", "nonfoil", 2), held("bolt-2xm", "Lightning Bolt", "foil", 1)];
        let exact = ListEntry::new(card("bolt-m11", "Lightning Bolt", &[("nonfoil", 1.0), ("foil", 5.0)]), 4, false, "");
        assert_eq!(exact.owned(&collection), 2);
        let any = ListEntry { any_printing: true, ..exact.clone() };
        assert_eq!(any.owned(&collection), 3);
        let foil = ListEntry { finish: "foil".into(), ..any.clone() };
        assert_eq!(foil.owned(&collection), 1);
        let order = ["nonfoil", "foil"];
        assert_eq!((exact.price(&order), foil.price(&order)), (Some(1.0), Some(5.0)));
        let etched_only = ListEntry::new(card("x", "X", &[("etched", 3.0), ("foil", 2.0)]), 1, false, "");
        assert_eq!(etched_only.price(&order), Some(2.0));
        assert_eq!(ListEntry::new(card("y", "Y", &[]), 1, false, "").price(&order), None);
    }

    #[test]
    fn entries_combine_and_validate() {
        let mut l = List { name: "  Burn  ".into(), ..List::default() };
        l.entries.push(ListEntry::new(held("bolt", "Lightning Bolt", "nonfoil", 3), 2, false, ""));
        l.entries.push(ListEntry::new(card("bolt", "Lightning Bolt", &[]), 2, false, ""));
        l.entries.push(ListEntry::new(card("bolt", "Lightning Bolt", &[]), 1, true, ""));
        l.validate().unwrap();
        assert_eq!(l.name, "Burn");
        assert_eq!(l.entries.iter().map(|e| e.quantity).collect::<Vec<_>>(), [4, 1]);
        assert!(l.entries[0].card.copies.is_empty(), "entries hold no copies");
        assert!(List { name: " ".into(), ..List::default() }.validate().is_err());
        let mut zero = l.clone();
        zero.entries[0].quantity = 0;
        assert!(zero.validate().is_err());
    }

    #[test]
    fn store_refresh_and_conflict_merge() {
        let dir = tempfile::tempdir().unwrap();
        let s = ListsStore::open(dir.path()).unwrap();
        let deck = s.create(List { name: "Deck".into(), ..List::default() }).unwrap();
        assert_eq!(deck.id.len(), 16);
        let deck = s
            .add_entries(
                &deck.id,
                vec![
                    ListEntry::new(card("bolt", "Lightning Bolt", &[("nonfoil", 1.0)]), 4, false, ""),
                    ListEntry::new(card("opt", "Opt", &[]), 2, true, ""),
                ],
            )
            .unwrap();
        assert_eq!(s.ids(), ["bolt", "opt"]);
        let updates =
            HashMap::from([("bolt".to_string(), CatalogUpdate { market: [("nonfoil".to_string(), 2.0)].into(), ..Default::default() })]);
        assert_eq!(s.update_catalog(&updates, "2026-11-14").unwrap(), 1);
        let bolt = &s.get(&deck.id).unwrap().entries[0].card;
        assert_eq!((bolt.market["nonfoil"], bolt.prices_updated.as_str()), (2.0, "2026-11-14"));

        // Theirs: Bolt at 3 (larger wins), Opt at 1 (ours, 2, stays), a new entry, a new list.
        let mut theirs = deck.clone();
        theirs.entries[0].quantity = 3;
        theirs.entries[1].quantity = 1;
        theirs.entries.push(ListEntry::new(card("shock", "Shock", &[]), 1, false, ""));
        let other = List { id: "other".into(), name: "Other".into(), ..List::default() };
        let mut bigger = deck.clone();
        bigger.entries[0].quantity = 6;
        assert_eq!(s.merge_copy(ListsData { lists: vec![theirs, other] }).unwrap(), 2);
        let merged = s.get(&deck.id).unwrap();
        assert_eq!(
            merged.entries.iter().map(|e| (e.card.id.as_str(), e.quantity)).collect::<Vec<_>>(),
            [("bolt", 4), ("opt", 2), ("shock", 1)]
        );
        assert_eq!(s.merge_copy(ListsData { lists: vec![bigger] }).unwrap(), 1);
        assert_eq!(s.get(&deck.id).unwrap().entries[0].quantity, 6);
        assert_eq!(s.all().iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["Deck", "Other"]);
        s.remove("other").unwrap();
        assert!(s.get("other").is_none());
        assert!(s.add_entries("nope", vec![]).is_err());
    }
}
