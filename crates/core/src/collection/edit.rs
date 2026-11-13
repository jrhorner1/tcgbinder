use std::collections::HashSet;

use serde::Serialize;

use super::copies::norm_finish;
use super::{Card, CardValue, Condition, filter::default_order};

/// Counts distinct printings (catalog IDs): several copies, finishes or conditions of
/// the same printing count once.
pub fn unique_cards<'a>(cards: impl IntoIterator<Item = &'a Card>) -> usize {
    cards.into_iter().map(|c| c.id.as_str()).collect::<HashSet<_>>().len()
}

/// Moves an owned card to another printing of the same card: catalog data comes from
/// `printing` and every purchase is kept. Copies in a finish the new printing doesn't
/// come in move to its first finish.
pub fn with_printing(owned: &Card, printing: &Card) -> Card {
    let mut c = printing.clone();
    c.copies = Default::default();
    for r in owned.purchase_rows(&[]) {
        let finish = match c.finishes.first() {
            Some(first) if !c.finishes.contains(&r.finish) => first.clone(),
            _ => r.finish,
        };
        c.add_purchase(&finish, r.condition, r.purchase);
    }
    if c.language.is_empty() {
        c.language = owned.language.clone();
    }
    c
}

/// Ranks priced cards by the value of one copy (their most valuable owned finish),
/// most valuable first.
pub fn top_by_each(cards: &[Card], n: usize) -> Vec<CardValue> {
    let mut out: Vec<CardValue> =
        cards.iter().filter_map(|c| c.best_price().map(|each| CardValue { card: c.clone(), each, total: c.market_value().0 })).collect();
    out.sort_by(|a, b| b.each.total_cmp(&a.each).then_with(|| default_order(&a.card, &b.card)));
    out.truncate(n);
    out
}

/// The cards of a set that aren't owned (by collector number).
pub fn missing(set_cards: &[Card], owned: &[Card]) -> Vec<Card> {
    let have: HashSet<&str> = owned.iter().map(|c| c.collector_number.as_str()).collect();
    set_cards.iter().filter(|c| !have.contains(c.collector_number.as_str())).cloned().collect()
}

/// An edit applied to many cards at once. `None` fields are left unchanged; each
/// applies to every copy of the card.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BulkChange {
    /// Move every copy to this condition.
    pub condition: Option<Condition>,
    /// Move every copy to this finish, if the printing comes in it.
    pub finish: Option<String>,
    pub price_paid: Option<f64>,
    pub acquired_date: Option<String>,
    pub delete: bool,
}

/// What a bulk edit did.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct BulkResult {
    pub updated: usize,
    pub deleted: usize,
    /// Cards the change didn't apply to, with why.
    pub skipped: Vec<String>,
}

impl BulkChange {
    /// Whether the change would do anything.
    pub fn is_empty(&self) -> bool {
        !self.delete && self.condition.is_none() && self.finish.is_none() && self.price_paid.is_none() && self.acquired_date.is_none()
    }

    /// Returns `c` with the change made (except delete), and why the finish change
    /// was skipped if the printing doesn't come in that finish.
    pub fn apply(&self, c: &Card) -> (Card, Option<String>) {
        let mut skipped = None;
        let mut finish = self.finish.as_deref().map(norm_finish);
        if let Some(f) = &finish
            && !c.finishes.is_empty()
            && !c.finishes.contains(f)
        {
            skipped = Some(format!("{} ({} · {}) isn't printed in {f}", c.name, c.set, c.collector_number));
            finish = None;
        }
        let mut out = c.clone();
        out.copies = Default::default();
        for mut r in c.purchase_rows(&[]) {
            if let Some(f) = &finish {
                r.finish = f.clone();
            }
            if let Some(cond) = self.condition {
                r.condition = cond;
            }
            if let Some(price) = self.price_paid {
                r.purchase.price_paid = price;
            }
            if let Some(date) = &self.acquired_date {
                r.purchase.acquired_date = date.clone();
            }
            out.add_purchase(&r.finish, r.condition, r.purchase); // groups that end up the same merge
        }
        (out, skipped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::Purchase;

    fn card(id: &str, number: &str, finishes: &[&str]) -> Card {
        let mut c = Card {
            id: id.into(),
            name: format!("Card {id}"),
            set: "M19".into(),
            collector_number: number.into(),
            finishes: finishes.iter().map(|s| s.to_string()).collect(),
            ..Card::default()
        };
        c.add_purchase(finishes[0], Condition::NearMint, Purchase { quantity: 1, price_paid: 1.0, ..Default::default() });
        c.add_purchase(finishes[0], Condition::LightPlay, Purchase { quantity: 2, price_paid: 2.0, ..Default::default() });
        c
    }

    #[test]
    fn bulk_change_merges_groups() {
        let c = card("a", "1", &["nonfoil", "foil"]);
        let change = BulkChange { condition: Some(Condition::HeavyPlay), price_paid: Some(0.5), ..Default::default() };
        let (out, skipped) = change.apply(&c);
        assert!(skipped.is_none());
        assert_eq!(out.purchases("nonfoil", Condition::HeavyPlay), [Purchase { quantity: 3, price_paid: 0.5, ..Default::default() }]);

        let (out, skipped) = BulkChange { finish: Some("Etched".into()), ..Default::default() }.apply(&c);
        assert!(skipped.unwrap().contains("isn't printed in etched"));
        assert_eq!(out.copies, c.copies);

        let (out, _) = BulkChange { finish: Some("foil".into()), ..Default::default() }.apply(&c);
        assert_eq!(out.finish_copy_count("foil"), 3);
    }

    #[test]
    fn printing_and_missing() {
        let owned = card("a", "1", &["foil"]);
        let printing = card("b", "7", &["nonfoil"]);
        let moved = with_printing(&owned, &printing);
        assert_eq!(moved.id, "b");
        assert_eq!(moved.finish_copy_count("nonfoil"), 3);

        let set = vec![card("x", "1", &["nonfoil"]), card("y", "2", &["nonfoil"])];
        assert_eq!(missing(&set, &[owned])[0].id, "y");
        assert_eq!(unique_cards(&set), 2);
    }
}
