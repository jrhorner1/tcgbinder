//! Sealed product: boxes, bundles and decks kept unopened. No free source prices
//! them, so the owner enters each item's value by hand. Kept in `sealed.json` beside
//! the collection.

use std::path::Path;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::collection::round_cents;
use crate::jsonfile::JsonFile;
use crate::lots::new_id;

/// The sealed product file in each game's data folder.
pub const SEALED_FILE: &str = "sealed.json";

/// Days after which a hand-entered value is marked as old.
pub const VALUE_STALE_DAYS: i64 = 90;

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

fn is_zero_i(v: &i64) -> bool {
    *v == 0
}

/// A product as the catalog lists it (MTGJSON's sealed products, for Magic).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SealedProduct {
    pub uuid: String,
    pub name: String,
    pub set: String,
    pub category: String,
    pub subtype: String,
    pub release_date: String,
    /// TCGplayer's product ID; 0 when unknown.
    pub tcgplayer_product_id: i64,
    /// A page where it can be bought, to look up a current price.
    pub purchase_url: String,
}

/// Sealed product owned: one product, bought for one price.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SealedItem {
    pub id: String,
    /// The catalog's ID for the product; "" for one typed by hand.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub product_uuid: String,
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub set: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub category: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub subtype: String,
    pub quantity: i64,
    /// Each; 0 when not recorded.
    #[serde(skip_serializing_if = "is_zero")]
    pub price_paid: f64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub acquired_date: String,
    /// Each, entered by the owner; 0 when not set.
    #[serde(skip_serializing_if = "is_zero")]
    pub value: f64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub value_date: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub note: String,
    /// TCGplayer's product ID, for a price source later; 0 when unknown.
    #[serde(skip_serializing_if = "is_zero_i")]
    pub tcgplayer_product_id: i64,
    /// A page where it can be bought, to look up a current price.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub purchase_url: String,
}

impl SealedItem {
    /// Checks the fields and tidies the text.
    pub fn validate(&mut self) -> Result<()> {
        self.name = self.name.trim().to_string();
        self.note = self.note.trim().to_string();
        self.set = self.set.trim().to_uppercase();
        if self.name.is_empty() {
            bail!("name the product");
        }
        if self.quantity <= 0 {
            bail!("the quantity must be at least 1");
        }
        if !(self.price_paid >= 0.0 && self.price_paid.is_finite() && self.value >= 0.0 && self.value.is_finite()) {
            bail!("prices can't be negative");
        }
        if self.value == 0.0 {
            self.value_date.clear();
        }
        Ok(())
    }

    /// Whether the value needs entering or updating: unset, or set more than
    /// [`VALUE_STALE_DAYS`] before `today`.
    pub fn value_stale(&self, today: &str) -> bool {
        if self.value <= 0.0 {
            return true;
        }
        let (Some(set), Some(now)) = (crate::dates::parse(&self.value_date), crate::dates::parse(today)) else { return true };
        (now - set).num_days() > VALUE_STALE_DAYS
    }
}

/// Totals for sealed items. Items without a value count toward `unvalued`, not
/// `value`; `paid` covers items with a price paid.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct SealedTotals {
    pub items: i64,
    pub paid: f64,
    pub value: f64,
    pub unvalued: i64,
    pub stale: usize,
}

pub fn sealed_totals(items: &[SealedItem], today: &str) -> SealedTotals {
    let mut t = SealedTotals::default();
    for i in items {
        t.items += i.quantity;
        t.paid += i.price_paid * i.quantity as f64;
        if i.value > 0.0 {
            t.value += i.value * i.quantity as f64;
        } else {
            t.unvalued += i.quantity;
        }
        if i.value_stale(today) {
            t.stale += 1;
        }
    }
    t.paid = round_cents(t.paid);
    t.value = round_cents(t.value);
    t
}

/// The sealed file's contents.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SealedData {
    pub items: Vec<SealedItem>,
}

/// One game's sealed product, kept in `sealed.json` (changed by other devices too,
/// through [`JsonFile`]).
pub struct SealedStore {
    file: JsonFile<SealedData>,
}

impl SealedStore {
    pub fn open(game_dir: &Path) -> Result<SealedStore> {
        Ok(SealedStore { file: JsonFile::open(game_dir.join(SEALED_FILE), true)? })
    }

    /// Every item, by set then name.
    pub fn all(&self) -> Vec<SealedItem> {
        let mut items = self.file.read(|d| d.items.clone());
        items.sort_by(|a, b| a.set.cmp(&b.set).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
        items
    }

    pub fn get(&self, id: &str) -> Option<SealedItem> {
        self.file.read(|d| d.items.iter().find(|i| i.id == id).cloned())
    }

    /// Adds an item, giving it an ID; returns it as saved.
    pub fn add(&self, mut item: SealedItem) -> Result<SealedItem> {
        item.validate()?;
        item.id = new_id();
        self.file.mutate(|d| {
            d.items.push(item.clone());
            Ok(())
        })?;
        Ok(item)
    }

    /// Saves a changed item (matched by ID); returns it as saved.
    pub fn save(&self, mut item: SealedItem) -> Result<SealedItem> {
        item.validate()?;
        self.file.mutate(|d| match d.items.iter_mut().find(|i| i.id == item.id) {
            Some(i) => {
                *i = item.clone();
                Ok(())
            }
            None => bail!("there is no sealed item {:?}", item.id),
        })?;
        Ok(item)
    }

    pub fn remove(&self, id: &str) -> Result<()> {
        self.file.mutate(|d| {
            d.items.retain(|i| i.id != id);
            Ok(())
        })
    }

    /// Merges a conflict copy: the union of both, by ID. Returns how many were added.
    pub fn merge_copy(&self, theirs: SealedData) -> Result<usize> {
        self.file.mutate(|d| {
            let mut added = 0;
            for item in theirs.items {
                if !d.items.iter().any(|i| i.id == item.id) {
                    d.items.push(item);
                    added += 1;
                }
            }
            Ok(added)
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

    fn item(name: &str, q: i64, paid: f64, value: f64, value_date: &str) -> SealedItem {
        SealedItem { name: name.into(), quantity: q, price_paid: paid, value, value_date: value_date.into(), ..SealedItem::default() }
    }

    #[test]
    fn validation_and_staleness() {
        let mut ok = item("  M19 Booster Box ", 1, 90.0, 0.0, "2026-01-01");
        ok.set = "m19".into();
        ok.validate().unwrap();
        assert_eq!((ok.name.as_str(), ok.set.as_str(), ok.value_date.as_str()), ("M19 Booster Box", "M19", ""), "no value, no date");
        assert!(item(" ", 1, 0.0, 0.0, "").validate().is_err());
        assert!(item("x", 0, 0.0, 0.0, "").validate().is_err());
        assert!(item("x", 1, -1.0, 0.0, "").validate().is_err());
        assert!(item("x", 1, 0.0, f64::NAN, "").validate().is_err());

        let today = "2026-11-14";
        assert!(item("x", 1, 0.0, 0.0, "").value_stale(today), "unset");
        assert!(!item("x", 1, 0.0, 5.0, "2026-08-16").value_stale(today), "90 days");
        assert!(item("x", 1, 0.0, 5.0, "2026-08-15").value_stale(today), "91 days");
        assert!(item("x", 1, 0.0, 5.0, "").value_stale(today), "no date");
    }

    #[test]
    fn totals() {
        let items = [item("a", 2, 100.0, 150.0, "2026-11-01"), item("b", 1, 0.0, 0.0, ""), item("c", 3, 10.0, 12.5, "2025-01-01")];
        let t = sealed_totals(&items, "2026-11-14");
        assert_eq!(t, SealedTotals { items: 6, paid: 230.0, value: 337.5, unvalued: 1, stale: 2 });
    }

    #[test]
    fn store_and_conflict_merge() {
        let dir = tempfile::tempdir().unwrap();
        let s = SealedStore::open(dir.path()).unwrap();
        let a = s.add(item("Bundle", 1, 40.0, 0.0, "")).unwrap();
        assert_eq!(a.id.len(), 16);
        let mut changed = a.clone();
        changed.value = 55.0;
        changed.value_date = "2026-11-14".into();
        s.save(changed).unwrap();
        assert_eq!(SealedStore::open(dir.path()).unwrap().get(&a.id).unwrap().value, 55.0);
        assert!(s.save(SealedItem { id: "nope".into(), ..item("x", 1, 0.0, 0.0, "") }).is_err());
        let theirs = SealedItem { id: "theirs".into(), ..item("Box", 1, 0.0, 0.0, "") };
        assert_eq!(s.merge_copy(SealedData { items: vec![a.clone(), theirs] }).unwrap(), 1);
        assert_eq!(s.all().iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["Box", "Bundle"]);
        s.remove(&a.id).unwrap();
        assert_eq!(s.all().len(), 1);
        let text = std::fs::read_to_string(s.path()).unwrap();
        assert!(!text.contains("purchase_url") && !text.contains("price_paid"), "empty fields stay out: {text}");
    }
}
