//! Sold cards: a ledger of sales in `sales.json` beside the collection. Selling takes
//! copies out of a purchase and records what they cost and what they sold for, so
//! earnings can be totalled even after the card has left the collection.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::collection::{Card, Condition, round_cents};
use crate::jsonfile::JsonFile;
use crate::lots::new_id;

/// The sales file in each game's data folder.
pub const SALES_FILE: &str = "sales.json";

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

/// A copy of a card's catalog fields, so a sale still reads after the card is gone.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CardRef {
    pub id: String,
    pub name: String,
    pub set: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub set_name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub collector_number: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub language: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub image_uri: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub rarity: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub released_at: String,
}

impl From<&Card> for CardRef {
    fn from(c: &Card) -> CardRef {
        CardRef {
            id: c.id.clone(),
            name: c.name.clone(),
            set: c.set.clone(),
            set_name: c.set_name.clone(),
            collector_number: c.collector_number.clone(),
            language: c.language.clone(),
            image_uri: c.image_uri.clone(),
            rarity: c.rarity.clone(),
            released_at: c.released_at.clone(),
        }
    }
}

impl CardRef {
    /// A card with these catalog fields and no copies, to put restored copies in
    /// when the card has left the collection.
    pub fn to_card(&self) -> Card {
        Card {
            id: self.id.clone(),
            name: self.name.clone(),
            set: self.set.clone(),
            set_name: self.set_name.clone(),
            collector_number: self.collector_number.clone(),
            language: self.language.clone(),
            image_uri: self.image_uri.clone(),
            rarity: self.rarity.clone(),
            released_at: self.released_at.clone(),
            ..Card::default()
        }
    }
}

/// Copies sold together: from one purchase, at one price, on one day.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sale {
    /// Random, 16 hexadecimal characters.
    pub id: String,
    /// `YYYY-MM-DD` sold.
    pub date: String,
    pub card: CardRef,
    pub finish: String,
    pub condition: Condition,
    pub quantity: i64,
    /// What one copy cost (the purchase's price paid); 0 when unknown.
    #[serde(skip_serializing_if = "is_zero")]
    pub cost_each: f64,
    /// The purchase's acquired date.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub acquired_date: String,
    /// What one copy sold for.
    pub sale_each: f64,
    /// Fees for the whole sale.
    #[serde(skip_serializing_if = "is_zero")]
    pub fees: f64,
    /// The note on the sale.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub note: String,
    /// The purchase's own note and lot, so undoing the sale restores it as it was.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub purchase_note: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub lot: String,
}

impl Sale {
    /// What the copies sold for, before fees.
    pub fn proceeds(&self) -> f64 {
        self.sale_each * self.quantity as f64
    }

    /// What the copies cost, if known.
    pub fn cost(&self) -> Option<f64> {
        (self.cost_each > 0.0).then_some(self.cost_each * self.quantity as f64)
    }

    /// Proceeds less fees and cost; unknown when the cost is.
    pub fn gain(&self) -> Option<f64> {
        self.cost().map(|cost| self.proceeds() - self.fees - cost)
    }

    /// Checks the fields a sale must have.
    pub fn validate(&self) -> Result<()> {
        if self.quantity <= 0 {
            bail!("sell at least one copy");
        }
        if !(self.sale_each >= 0.0 && self.sale_each.is_finite()) {
            bail!("the sale price can't be negative");
        }
        if !(self.fees >= 0.0 && self.fees.is_finite()) {
            bail!("fees can't be negative");
        }
        if self.date.len() != 10 {
            bail!("the sale date must look like 2024-03-15");
        }
        Ok(())
    }
}

/// Totals for some sales. Sales whose cost is unknown count toward proceeds and fees
/// but not cost or gain; `unknown_cost_copies` says how many copies that is.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct SalesTotals {
    pub sales: usize,
    pub copies: i64,
    pub proceeds: f64,
    pub fees: f64,
    pub cost: f64,
    pub gain: f64,
    pub unknown_cost_copies: i64,
}

impl SalesTotals {
    fn add(&mut self, s: &Sale) {
        self.sales += 1;
        self.copies += s.quantity;
        self.proceeds += s.proceeds();
        self.fees += s.fees;
        match s.gain() {
            Some(g) => {
                self.cost += s.cost().unwrap_or(0.0);
                self.gain += g;
            }
            None => self.unknown_cost_copies += s.quantity,
        }
    }

    fn rounded(mut self) -> SalesTotals {
        self.proceeds = round_cents(self.proceeds);
        self.fees = round_cents(self.fees);
        self.cost = round_cents(self.cost);
        self.gain = round_cents(self.gain);
        self
    }
}

/// The totals of `sales`, and per month (`YYYY-MM`, in order).
pub fn sales_totals<'a>(sales: impl IntoIterator<Item = &'a Sale>) -> (SalesTotals, BTreeMap<String, SalesTotals>) {
    let mut all = SalesTotals::default();
    let mut months: BTreeMap<String, SalesTotals> = BTreeMap::new();
    for s in sales {
        all.add(s);
        months.entry(s.date.get(..7).unwrap_or("").to_string()).or_default().add(s);
    }
    (all.rounded(), months.into_iter().map(|(m, t)| (m, t.rounded())).collect())
}

/// The sales file's contents.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SalesData {
    pub sales: Vec<Sale>,
}

/// One game's sales, kept in `sales.json` (changed by other devices too, through
/// [`JsonFile`]).
pub struct SalesStore {
    file: JsonFile<SalesData>,
}

impl SalesStore {
    /// Opens (or starts) the sales file in a game's data folder.
    pub fn open(game_dir: &Path) -> Result<SalesStore> {
        Ok(SalesStore { file: JsonFile::open(game_dir.join(SALES_FILE), true)? })
    }

    /// Every sale, newest first.
    pub fn all(&self) -> Vec<Sale> {
        let mut sales = self.file.read(|d| d.sales.clone());
        sales.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.card.name.cmp(&b.card.name)));
        sales
    }

    pub fn get(&self, id: &str) -> Option<Sale> {
        self.file.read(|d| d.sales.iter().find(|s| s.id == id).cloned())
    }

    /// Records a sale, giving it an ID, and returns it.
    pub fn add(&self, mut sale: Sale) -> Result<Sale> {
        sale.validate()?;
        sale.id = new_id();
        self.file.mutate(|d| {
            d.sales.push(sale.clone());
            Ok(())
        })?;
        Ok(sale)
    }

    /// Deletes a sale.
    pub fn remove(&self, id: &str) -> Result<()> {
        self.file.mutate(|d| {
            d.sales.retain(|s| s.id != id);
            Ok(())
        })
    }

    /// Merges a conflict copy of the sales file: the union of both, by ID. Returns
    /// how many sales were added.
    pub fn merge_copy(&self, theirs: SalesData) -> Result<usize> {
        self.file.mutate(|d| {
            let mut added = 0;
            for sale in theirs.sales {
                if !d.sales.iter().any(|s| s.id == sale.id) {
                    d.sales.push(sale);
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

    /// The sales file's path.
    pub fn path(&self) -> &Path {
        self.file.path()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sale(date: &str, q: i64, cost: f64, each: f64, fees: f64) -> Sale {
        Sale {
            date: date.into(),
            card: CardRef { id: "c".into(), name: "Opt".into(), ..CardRef::default() },
            finish: "nonfoil".into(),
            quantity: q,
            cost_each: cost,
            sale_each: each,
            fees,
            ..Sale::default()
        }
    }

    #[test]
    fn totals_leave_unknown_costs_out_of_the_gain() {
        let sales = [sale("2026-01-05", 2, 1.0, 3.0, 0.5), sale("2026-01-20", 1, 0.0, 10.0, 1.0), sale("2026-03-01", 4, 0.25, 0.1, 0.0)];
        assert_eq!(sales[0].gain(), Some(3.5));
        assert_eq!(sales[1].gain(), None);
        let (all, months) = sales_totals(&sales);
        assert_eq!(
            all,
            SalesTotals { sales: 3, copies: 7, proceeds: 16.4, fees: 1.5, cost: 3.0, gain: 2.9, unknown_cost_copies: 1 },
            "gain: 3.5 + (0.4 - 1.0); the unknown-cost sale is left out"
        );
        assert_eq!(months.keys().collect::<Vec<_>>(), ["2026-01", "2026-03"]);
        assert_eq!((months["2026-01"].proceeds, months["2026-01"].gain), (16.0, 3.5));
        assert_eq!(months["2026-03"].gain, -0.6);
    }

    #[test]
    fn validation() {
        assert!(sale("2026-01-05", 1, 0.0, 1.0, 0.0).validate().is_ok());
        assert!(sale("2026-01-05", 0, 0.0, 1.0, 0.0).validate().is_err());
        assert!(sale("2026-01-05", 1, 0.0, -1.0, 0.0).validate().is_err());
        assert!(sale("2026-01-05", 1, 0.0, 1.0, -1.0).validate().is_err());
        assert!(sale("", 1, 0.0, 1.0, 0.0).validate().is_err());
    }

    #[test]
    fn store_and_conflict_merge() {
        let dir = tempfile::tempdir().unwrap();
        let s = SalesStore::open(dir.path()).unwrap();
        let a = s.add(sale("2026-01-05", 1, 0.0, 1.0, 0.0)).unwrap();
        let b = s.add(sale("2026-02-05", 1, 0.0, 1.0, 0.0)).unwrap();
        assert_eq!(a.id.len(), 16);
        assert_eq!(
            SalesStore::open(dir.path()).unwrap().all().iter().map(|x| x.id.clone()).collect::<Vec<_>>(),
            [b.id.clone(), a.id.clone()]
        );
        let theirs = Sale { id: "theirs".into(), ..sale("2026-03-01", 2, 0.0, 1.0, 0.0) };
        assert_eq!(s.merge_copy(SalesData { sales: vec![a.clone(), theirs] }).unwrap(), 1);
        assert_eq!(s.all().len(), 3);
        s.remove(&a.id).unwrap();
        assert!(s.get(&a.id).is_none());
        // Empty fields stay out of the file.
        let text = std::fs::read_to_string(s.path()).unwrap();
        assert!(!text.contains("purchase_note") && !text.contains("cost_each"));
    }
}
