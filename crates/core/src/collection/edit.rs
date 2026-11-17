//! Changes to cards: moving a card to another printing, bulk edits (`BulkChange`),
//! and small helpers over many cards (unique cards, the top cards, a set's missing cards).

use std::collections::HashSet;

use anyhow::{Context, Result};
use serde::Serialize;

use super::copies::norm_finish;
use super::tags::{is_system_tag, normalize_tags, same_tag};
use super::{Card, CardValue, Condition, filter::default_order, in_printed_set};

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
    for r in owned.purchase_rows() {
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

/// The cards of a set that aren't owned (by collector number). With a `printed_size`,
/// only those in the printed run (see [`in_printed_set`]).
pub fn missing(set_cards: &[Card], owned: &[Card], printed_size: Option<usize>) -> Vec<Card> {
    let have: HashSet<&str> = owned.iter().map(|c| c.collector_number.as_str()).collect();
    set_cards
        .iter()
        .filter(|c| printed_size.is_none_or(|n| in_printed_set(&c.collector_number, n)))
        .filter(|c| !have.contains(c.collector_number.as_str()))
        .cloned()
        .collect()
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
    /// Tags to put on every purchase, take off it, or (`clear_tags`) remove all the
    /// user's tags from first (system tags, such as a lot's, stay). Applied in that
    /// order: clear, remove, add.
    pub add_tags: Vec<String>,
    pub remove_tags: Vec<String>,
    pub clear_tags: bool,
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
        !self.delete
            && self.condition.is_none()
            && self.finish.is_none()
            && self.price_paid.is_none()
            && self.acquired_date.is_none()
            && self.add_tags.is_empty()
            && self.remove_tags.is_empty()
            && !self.clear_tags
    }

    /// Returns `c` with the change made (except delete), and why the finish change
    /// was skipped if the printing doesn't come in that finish. Fails, naming the
    /// card, if a purchase would end up with too many tags.
    pub fn apply(&self, c: &Card) -> Result<(Card, Option<String>)> {
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
        for mut r in c.purchase_rows() {
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
            r.purchase.tags =
                self.tags_for(&r.purchase.tags).with_context(|| format!("{} ({} · {})", c.name, c.set, c.collector_number))?;
            out.add_purchase(&r.finish, r.condition, r.purchase); // groups that end up the same merge
        }
        Ok((out, skipped))
    }

    /// A purchase's tags after the change: clear, then remove, then add.
    fn tags_for(&self, tags: &[String]) -> Result<Vec<String>> {
        if !self.clear_tags && self.remove_tags.is_empty() && self.add_tags.is_empty() {
            return Ok(tags.to_vec());
        }
        let mut out: Vec<String> =
            if self.clear_tags { tags.iter().filter(|t| is_system_tag(t)).cloned().collect() } else { tags.to_vec() };
        out.retain(|t| !self.remove_tags.iter().any(|r| same_tag(r, t)));
        out.extend(self.add_tags.iter().cloned());
        normalize_tags(&out)
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
        let (out, skipped) = change.apply(&c).unwrap();
        assert!(skipped.is_none());
        assert_eq!(out.purchases("nonfoil", Condition::HeavyPlay), [Purchase { quantity: 3, price_paid: 0.5, ..Default::default() }]);

        let (out, skipped) = BulkChange { finish: Some("Etched".into()), ..Default::default() }.apply(&c).unwrap();
        assert!(skipped.unwrap().contains("isn't printed in etched"));
        assert_eq!(out.copies, c.copies);

        let (out, _) = BulkChange { finish: Some("foil".into()), ..Default::default() }.apply(&c).unwrap();
        assert_eq!(out.finish_copy_count("foil"), 3);
    }

    #[test]
    fn printing_and_missing() {
        let owned = card("a", "1", &["foil"]);
        let printing = card("b", "7", &["nonfoil"]);
        let moved = with_printing(&owned, &printing);
        assert_eq!(moved.id, "b");
        assert_eq!(moved.finish_copy_count("nonfoil"), 3);

        let set = vec![card("x", "1", &["nonfoil"]), card("y", "2", &["nonfoil"]), card("z", "3a", &["nonfoil"])];
        assert_eq!(missing(&set, std::slice::from_ref(&owned), None).iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["y", "z"]);
        assert_eq!(missing(&set, &[owned], Some(2)).iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["y"], "the printed run only");
        assert_eq!(unique_cards(&set), 3);
    }

    fn tags_of(c: &Card) -> Vec<(i64, Vec<String>)> {
        let mut out: Vec<(i64, Vec<String>)> = c.purchase_rows().into_iter().map(|r| (r.purchase.quantity, r.purchase.tags)).collect();
        out.sort();
        out
    }

    fn v(t: &[&str]) -> Vec<String> {
        t.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn bulk_tags_clear_remove_add_then_merge() {
        // Two purchases (NM ×1 at $1, LP ×2 at $2), tagged differently.
        let mut c = card("a", "1", &["nonfoil"]);
        for (i, list) in c.copies.0.values_mut().flat_map(|m| m.values_mut()).enumerate() {
            list[0].tags = if i == 0 { v(&["binder A", "old"]) } else { v(&["old"]) };
        }
        let change = |add: &[&str], remove: &[&str], clear: bool| BulkChange {
            add_tags: v(add),
            remove_tags: v(remove),
            clear_tags: clear,
            ..Default::default()
        };
        let (out, _) = change(&["deck"], &["OLD"], false).apply(&c).unwrap();
        assert_eq!(tags_of(&out), [(1, v(&["binder A", "deck"])), (2, v(&["deck"]))], "remove ignores case");
        let (out, _) = change(&["deck"], &[], true).apply(&c).unwrap();
        assert_eq!(tags_of(&out), [(1, v(&["deck"])), (2, v(&["deck"]))], "clear before add");
        let (out, _) = change(&["old"], &["old"], false).apply(&c).unwrap();
        assert_eq!(tags_of(&out), [(1, v(&["binder A", "old"])), (2, v(&["old"]))], "remove before add");
        // Made identical (same tags, then same price): they merge.
        let merge = BulkChange { price_paid: Some(1.0), condition: Some(Condition::NearMint), clear_tags: true, ..Default::default() };
        assert_eq!(tags_of(&merge.apply(&c).unwrap().0), [(3, vec![])]);
        assert!(!change(&[], &[], true).is_empty() && !change(&["x"], &[], false).is_empty() && !change(&[], &["x"], false).is_empty());

        // 19 tags already: adding 2 more fails, naming the card.
        let mut full = card("b", "2", &["nonfoil"]);
        for list in full.copies.0.values_mut().flat_map(|m| m.values_mut()) {
            list[0].tags = (0..19).map(|i| format!("t{i:02}")).collect();
        }
        let err = change(&["x", "y"], &[], false).apply(&full).unwrap_err();
        assert!(format!("{err:#}").contains("Card b") && format!("{err:#}").contains("at most 20"), "{err:#}");
    }

    #[test]
    fn tags_move_with_their_copies() {
        let mut owned = card("a", "1", &["foil"]);
        for list in owned.copies.0.values_mut().flat_map(|m| m.values_mut()) {
            list[0].tags = v(&["deck"]);
        }
        let moved = with_printing(&owned, &card("b", "7", &["nonfoil"]));
        assert_eq!(tags_of(&moved), tags_of(&owned), "changing printing keeps tags");
        let (refinished, _) =
            BulkChange { finish: Some("foil".into()), condition: Some(Condition::Moderate), ..Default::default() }.apply(&owned).unwrap();
        assert_eq!(tags_of(&refinished), [(1, v(&["deck"])), (2, v(&["deck"]))], "a finish or condition change keeps tags");
        assert_eq!(refinished.finish_copy_count("foil"), 3);
    }
}
