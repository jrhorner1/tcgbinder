//! Changing many cards at once: a bulk edit, and merging a conflict copy of the
//! collection file saved by another device.

use anyhow::Result;

use super::{Store, card_key, normalize_card};
use crate::collection::tags::{clean_user_tag, is_system_tag, normalize_user_tags, tags_in_use, with_spellings};
use crate::collection::{BulkChange, BulkResult, Card};

impl Store {
    /// Applies `change` to the cards with the given keys and saves.
    /// Applies `change` to the cards with the given keys and saves. If the change
    /// can't be made to any card (too many tags), nothing is saved.
    pub fn bulk_edit(&self, keys: &[String], change: &BulkChange) -> Result<BulkResult> {
        let mut change = change.clone();
        change.add_tags = normalize_user_tags(&change.add_tags)?;
        // Tags to remove aren't checked beyond this: the app's own can't be removed.
        for t in change.remove_tags.iter().filter(|t| is_system_tag(t.trim())) {
            clean_user_tag(t)?;
        }
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

    /// Merges the cards of a conflict copy of this collection (the same file, saved by
    /// another device at the same time), without counting anything twice: the union of
    /// both files' cards; for a card in both, the union of their purchases, taking the
    /// larger quantity where both have the same purchase (finish, condition, price paid,
    /// acquired date and tags). A card or purchase deleted on one side comes back. Returns
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::store::tests::{card, tagged, tags_of, v};
    use crate::collection::{CatalogUpdate, Condition, Purchase};

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
}
