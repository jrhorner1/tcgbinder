use std::cmp::Ordering;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{Card, Condition};

/// One purchase: copies of a printing in one finish and condition, bought for the same
/// price on the same date.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Purchase {
    pub quantity: i64,
    /// Per copy; 0 when not recorded.
    #[serde(skip_serializing_if = "is_zero")]
    pub price_paid: f64,
    /// `YYYY-MM-DD`, or "" when not recorded.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub acquired_date: String,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

/// The copies of one printing owned: finish → condition → purchases. Each purchase
/// keeps its own price paid and acquired date.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Copies(pub BTreeMap<String, BTreeMap<Condition, Vec<Purchase>>>);

/// One purchase of a card, with its finish and condition.
#[derive(Clone, Debug, PartialEq)]
pub struct PurchaseRow {
    pub finish: String,
    pub condition: Condition,
    pub purchase: Purchase,
}

pub(crate) fn norm_finish(f: &str) -> String {
    f.trim().to_lowercase()
}

impl Copies {
    /// Whether no copies are held.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Adds a purchase. Copies bought for the same price on the same date are one
    /// purchase, so a matching one grows instead.
    pub fn add(&mut self, finish: &str, cond: Condition, p: Purchase) {
        if p.quantity <= 0 {
            return;
        }
        let list = self.0.entry(norm_finish(finish)).or_default().entry(cond).or_default();
        match list.iter_mut().find(|cur| cur.price_paid == p.price_paid && cur.acquired_date == p.acquired_date) {
            Some(cur) => cur.quantity += p.quantity,
            None => list.push(p),
        }
    }

    /// Every purchase with copies, in map order. Totals use it rather than
    /// [`Card::purchase_rows`], which allocates and sorts.
    pub fn each(&self) -> impl Iterator<Item = (&str, Condition, &Purchase)> {
        self.0.iter().flat_map(|(f, by_cond)| {
            by_cond.iter().flat_map(move |(c, list)| list.iter().filter(|p| p.quantity > 0).map(move |p| (f.as_str(), *c, p)))
        })
    }
}

/// Orders purchases by date (undated last), then price.
fn purchase_order(a: &Purchase, b: &Purchase) -> Ordering {
    if a.acquired_date != b.acquired_date {
        if a.acquired_date.is_empty() || b.acquired_date.is_empty() {
            return if a.acquired_date.is_empty() { Ordering::Greater } else { Ordering::Less };
        }
        return a.acquired_date.cmp(&b.acquired_date);
    }
    a.price_paid.total_cmp(&b.price_paid)
}

impl Card {
    /// Adds a purchase. It joins an existing purchase in the same finish and condition
    /// when the price paid and acquired date match.
    pub fn add_purchase(&mut self, finish: &str, cond: Condition, p: Purchase) {
        self.copies.add(finish, cond, p);
    }

    /// The purchases in one finish and condition, oldest first.
    pub fn purchases(&self, finish: &str, cond: Condition) -> Vec<Purchase> {
        let mut out = self.copies.0.get(&norm_finish(finish)).and_then(|m| m.get(&cond)).cloned().unwrap_or_default();
        out.sort_by(purchase_order);
        out
    }

    /// The card's copies: by finish name, then conditions best first, then purchases
    /// oldest first.
    pub fn purchase_rows(&self) -> Vec<PurchaseRow> {
        let mut out: Vec<PurchaseRow> =
            self.copies.each().map(|(f, c, p)| PurchaseRow { finish: f.to_string(), condition: c, purchase: p.clone() }).collect();
        // Stable: the copies already come by finish, then condition.
        out.sort_by(|a, b| {
            a.finish.cmp(&b.finish).then_with(|| a.condition.cmp(&b.condition)).then_with(|| purchase_order(&a.purchase, &b.purchase))
        });
        out
    }

    /// The total number of copies owned.
    pub fn copy_count(&self) -> i64 {
        self.copies.each().map(|(_, _, p)| p.quantity).sum()
    }

    /// The number of copies owned in one finish.
    pub fn finish_copy_count(&self, finish: &str) -> i64 {
        self.copies.0.get(&norm_finish(finish)).map_or(0, |m| m.values().flatten().map(|p| p.quantity).sum())
    }

    /// The finishes with copies, in `finish_order` (others after, by name).
    pub fn owned_finishes(&self, finish_order: &[&str]) -> Vec<String> {
        let mut out: Vec<String> = self.copies.0.keys().filter(|f| self.has_finish(f)).cloned().collect();
        out.sort_by_key(|f| finish_order.iter().position(|o| o == f).unwrap_or(finish_order.len()));
        out
    }

    /// The total price paid for copies with a known price, and the number of copies
    /// without one.
    pub fn price_paid(&self) -> (f64, i64) {
        let (mut total, mut unknown) = (0.0, 0);
        for (_, _, p) in self.copies.each() {
            if p.price_paid > 0.0 {
                total += p.price_paid * p.quantity as f64;
            } else {
                unknown += p.quantity;
            }
        }
        (total, unknown)
    }

    /// The market value of every copy (each at its finish's price), and the number of
    /// copies whose finish has no price.
    pub fn market_value(&self) -> (f64, i64) {
        let (mut total, mut unpriced) = (0.0, 0);
        for (f, _, p) in self.copies.each() {
            match self.market.get(f) {
                Some(price) => total += price * p.quantity as f64,
                None => unpriced += p.quantity,
            }
        }
        (total, unpriced)
    }

    /// The highest market price among the finishes owned: one copy's value.
    pub fn best_price(&self) -> Option<f64> {
        self.copies.0.keys().filter_map(|f| self.market.get(f).copied()).reduce(f64::max)
    }

    /// Whether any copies are owned in `finish` (case-insensitive).
    pub fn has_finish(&self, finish: &str) -> bool {
        self.finish_copy_count(finish) > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(q: i64, price: f64, date: &str) -> Purchase {
        Purchase { quantity: q, price_paid: price, acquired_date: date.into() }
    }

    #[test]
    fn purchases_merge_and_sort() {
        let mut c = Card::default();
        c.add_purchase("Foil", Condition::NearMint, p(1, 2.0, "2024-02-01"));
        c.add_purchase("foil", Condition::NearMint, p(2, 2.0, "2024-02-01"));
        c.add_purchase("foil", Condition::NearMint, p(1, 1.0, ""));
        c.add_purchase("foil", Condition::NearMint, p(1, 3.0, "2023-01-01"));
        c.add_purchase("nonfoil", Condition::LightPlay, p(4, 0.0, ""));
        c.add_purchase("nonfoil", Condition::LightPlay, p(0, 9.0, ""));
        assert_eq!(c.copy_count(), 9);
        assert_eq!(c.finish_copy_count("FOIL"), 5);
        let got = c.purchases("foil", Condition::NearMint);
        assert_eq!(got, [p(1, 3.0, "2023-01-01"), p(3, 2.0, "2024-02-01"), p(1, 1.0, "")]);
        let rows = c.purchase_rows();
        assert_eq!((rows[0].finish.as_str(), rows[0].purchase.quantity), ("foil", 1));
        assert_eq!(rows.last().unwrap().finish, "nonfoil");
        assert_eq!(c.owned_finishes(&["nonfoil", "foil"]), ["nonfoil", "foil"]);
        assert_eq!(c.owned_finishes(&[]), ["foil", "nonfoil"]);
    }

    #[test]
    fn totals() {
        let mut c = Card::default();
        c.market.insert("nonfoil".into(), 1.5);
        c.market.insert("foil".into(), 4.0);
        c.add_purchase("nonfoil", Condition::NearMint, p(2, 1.0, ""));
        c.add_purchase("etched", Condition::NearMint, p(1, 0.0, ""));
        assert_eq!(c.price_paid(), (2.0, 1));
        assert_eq!(c.market_value(), (3.0, 1));
        assert_eq!(c.best_price(), Some(1.5));
    }
}
