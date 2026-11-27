//! Tags on the collection's purchases: the tags in use, renaming and deleting one,
//! and the general operations lots are built on (a lot is the tag `lot:<id>`):
//! finding, adding and taking off a tag, and pricing a lot's copies.

use anyhow::{Result, bail};

use super::Store;
use crate::collection::tags::lot_of;
use crate::collection::tags::{
    LOT_PREFIX, SYSTEM_PREFIXES, card_has_tag, clean_user_tag, is_system_tag, lot_tag, normalize_tags, same_tag, tags_in_use,
    with_spellings,
};
use crate::collection::{Card, Purchase};
use crate::lots::{LotCopy, LotPrices};

/// A card whose copies get a tag ([`Store::tag_copies`]): all its copies without the
/// tag, or `quantity` of them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CopyPick {
    pub key: String,
    pub quantity: Option<i64>,
}

/// Copies still marked as bought in a lot whose record is gone (deleted, or lost with an
/// older backup), by lot.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct LostLot {
    /// The lot's ID, from the copies' `lot:<id>` tag.
    pub id: String,
    /// Printings.
    pub cards: usize,
    pub copies: i64,
    /// What the copies were costed at, all together.
    pub paid: f64,
    /// The earliest and latest acquired dates among them ("" if none).
    pub first_date: String,
    pub last_date: String,
    /// A few of the cards' names, to recognize the lot by.
    pub names: Vec<String>,
}

/// What renaming or deleting a tag did: purchases changed, and the backup taken
/// first (None when the tag wasn't in use, so nothing was done).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TagEdit {
    pub changed: usize,
    pub backup: Option<String>,
}

impl Store {
    /// A lot's copies, one per card, finish and condition (purchases with different
    /// notes added together), for working out the lot's prices.
    pub fn lot_copies(&self, lot_id: &str, is_basic_land: impl Fn(&Card) -> bool) -> Vec<LotCopy> {
        let tag = lot_tag(lot_id);
        self.file.read(|data| {
            let mut out: Vec<LotCopy> = Vec::new();
            for c in data.cards.values() {
                for (finish, condition, p) in c.copies.each().filter(|(_, _, p)| has_tag(p, &tag)) {
                    match out.iter_mut().find(|x| x.card_id == c.id && x.finish == finish && x.condition == condition) {
                        Some(x) => x.quantity += p.quantity,
                        None => out.push(LotCopy {
                            card_id: c.id.clone(),
                            finish: finish.to_string(),
                            condition,
                            quantity: p.quantity,
                            market: c.market_price(finish),
                            rarity: c.rarity.clone(),
                            basic_land: is_basic_land(c),
                        }),
                    }
                }
            }
            out
        })
    }

    /// The lots copies are marked as bought in that `exists` says aren't there, with
    /// what they hold; in ID order.
    pub fn lost_lots(&self, exists: impl Fn(&str) -> bool) -> Vec<LostLot> {
        let mut out: std::collections::BTreeMap<String, LostLot> = Default::default();
        self.file.read(|data| {
            for c in data.cards.values() {
                let mut counted = std::collections::HashSet::new();
                for (_, _, p) in c.copies.each() {
                    let Some(id) = lot_of(&p.tags).filter(|id| !exists(id)) else { continue };
                    let l = out.entry(id.to_string()).or_insert_with(|| LostLot { id: id.to_string(), ..LostLot::default() });
                    if counted.insert(id.to_string()) {
                        l.cards += 1;
                        if l.names.len() < 3 {
                            l.names.push(c.name.clone());
                        }
                    }
                    l.copies += p.quantity;
                    l.paid += p.price_paid * p.quantity as f64;
                    let d = &p.acquired_date;
                    if !d.is_empty() && (l.first_date.is_empty() || *d < l.first_date) {
                        l.first_date = d.clone();
                    }
                    if *d > l.last_date {
                        l.last_date = d.clone();
                    }
                }
            }
        });
        out.into_values()
            .map(|mut l| {
                l.paid = (l.paid * 100.0).round() / 100.0;
                l
            })
            .collect()
    }

    /// Sets the price paid (from `prices`) and acquired date of every copy in a lot, in
    /// one save. Returns the number of cards changed.
    pub fn reprice_lot(&self, lot_id: &str, prices: &LotPrices, acquired_date: &str) -> Result<usize> {
        let tag = lot_tag(lot_id);
        self.file.mutate(|data| {
            let mut changed = 0;
            let mut extra = prices.extra.clone();
            for c in data.cards.values_mut().filter(|c| card_has_tag(c, &tag)) {
                let mut copies = super::Copies::default();
                for (finish, cond, p) in c.copies.each() {
                    let mut p = p.clone();
                    if has_tag(&p, &tag) {
                        let key = (c.id.clone(), finish.to_string(), cond);
                        if let Some(price) = prices.each.get(&key) {
                            p.price_paid = *price;
                        }
                        p.acquired_date = acquired_date.to_string();
                        // The rounding remainder goes on one copy of its card.
                        if let Some((_, more)) = extra.take_if(|(k, _)| *k == key) {
                            let one = super::Purchase { quantity: 1, price_paid: crate::lots::round4(p.price_paid + more), ..p.clone() };
                            p.quantity -= 1;
                            copies.add(finish, cond, one);
                        }
                    }
                    copies.add(finish, cond, p);
                }
                c.copies = copies;
                changed += 1;
            }
            Ok(changed)
        })
    }

    /// Every tag in use, sorted ignoring case.
    pub fn tag_names(&self) -> Vec<String> {
        self.file.read(|data| tags_in_use(data.cards.values()))
    }

    /// `tags` with each replaced by the spelling already in the collection when one
    /// matches ignoring case.
    pub fn canonical_tags(&self, tags: Vec<String>) -> Vec<String> {
        with_spellings(tags, &self.tag_names())
    }

    /// Renames a tag on every purchase that has it (after a backup). If `to` is
    /// already a tag, the two become one with `to`'s existing spelling, unless `from`
    /// and `to` differ only in case, when `to`'s new spelling is used everywhere.
    /// Purchases made identical merge.
    pub fn rename_tag(&self, from: &str, to: &str) -> Result<TagEdit> {
        if is_system_tag(from) {
            bail!("the app's own tags can't be renamed");
        }
        let to = clean_user_tag(to)?;
        let target = if same_tag(from, &to) { to } else { self.tag_names().into_iter().find(|t| same_tag(t, &to)).unwrap_or(to) };
        self.edit_tag(from, "tag-rename", |tags| tags.push(target.clone()))
    }

    /// Removes a tag from every purchase that has it (after a backup).
    pub fn delete_tag(&self, name: &str) -> Result<TagEdit> {
        if is_system_tag(name) {
            bail!("the app's own tags can't be deleted");
        }
        self.edit_tag(name, "tag-delete", |_| {})
    }

    /// Takes `name` off every purchase that has it, lets `then` change those
    /// purchases' tags, and merges purchases made identical. Backs up first when the
    /// tag is in use.
    fn edit_tag(&self, name: &str, reason: &str, then: impl Fn(&mut Vec<String>)) -> Result<TagEdit> {
        if !self.tag_names().iter().any(|t| same_tag(t, name)) {
            return Ok(TagEdit::default());
        }
        let backup = self.backup(reason)?;
        let changed = self.file.mutate(|data| {
            let mut changed = 0;
            for c in data.cards.values_mut() {
                let mut copies = super::Copies::default();
                for (f, cond, p) in c.copies.each() {
                    let mut p = p.clone();
                    if p.tags.iter().any(|t| same_tag(t, name)) {
                        p.tags.retain(|t| !same_tag(t, name));
                        then(&mut p.tags);
                        p.tags = normalize_tags(&p.tags)?;
                        changed += 1;
                    }
                    copies.add(f, cond, p);
                }
                c.copies = copies;
            }
            Ok(changed)
        })?;
        Ok(TagEdit { changed, backup: Some(backup) })
    }

    /// The cards with a purchase carrying `tag`.
    pub fn cards_with_tag(&self, tag: &str) -> Vec<Card> {
        self.cards_where(|c| card_has_tag(c, tag))
    }

    /// Takes `tag` off the purchases of the card stored under `key`: its copies stay,
    /// with their price and date. Returns the copies the tag covered.
    pub fn untag_card(&self, tag: &str, key: &str) -> Result<i64> {
        self.file.mutate(|data| {
            let Some(card) = data.cards.get_mut(key) else { bail!("that card isn't in the collection") };
            let mut freed = 0;
            let mut copies = super::Copies::default();
            for (f, cond, p) in card.copies.each() {
                if has_tag(p, tag) {
                    freed += p.quantity;
                    copies.add(f, cond, without_tag(p, tag));
                } else {
                    copies.add(f, cond, p.clone());
                }
            }
            if freed == 0 {
                if tag.starts_with(LOT_PREFIX) {
                    bail!("{} isn't in that lot", card.name);
                }
                bail!("{} doesn't have the tag {tag:?}", card.name);
            }
            card.copies = copies;
            Ok(freed)
        })
    }

    /// Takes `tag` off every purchase; the copies stay, with their prices. Returns the
    /// cards changed.
    pub fn untag_all(&self, tag: &str) -> Result<usize> {
        self.edit_tagged(tag, |p| Some(without_tag(p, tag)))
    }

    /// Deletes every purchase carrying `tag` (a card left with no copies goes too).
    /// Returns the cards changed.
    pub fn remove_tagged(&self, tag: &str) -> Result<usize> {
        self.edit_tagged(tag, |_| None)
    }

    /// Replaces each purchase carrying `tag` with what `f` makes of it (None deletes
    /// it), in one save, and drops cards left with no copies. Returns the cards changed.
    fn edit_tagged(&self, tag: &str, f: impl Fn(&Purchase) -> Option<Purchase>) -> Result<usize> {
        self.file.mutate(|data| {
            let mut changed = 0;
            for c in data.cards.values_mut().filter(|c| card_has_tag(c, tag)) {
                let mut copies = super::Copies::default();
                for (finish, cond, p) in c.copies.each() {
                    let next = if has_tag(p, tag) { f(p) } else { Some(p.clone()) };
                    if let Some(p) = next {
                        copies.add(finish, cond, p);
                    }
                }
                c.copies = copies;
                changed += 1;
            }
            data.cards.retain(|_, c| c.copy_count() > 0);
            Ok(changed)
        })
    }

    /// Puts `tag` on copies the collection already holds, in one save: for each pick,
    /// every copy of the card without the tag, or `quantity` of them. When the tag has
    /// a system prefix (a lot's), it replaces any other tag with that prefix: copies
    /// carrying none are taken first, and the tags replaced are returned (the lots the
    /// copies left, which need repricing). `edit` is applied to each purchase moved (a
    /// lot sets its date). Returns the copies tagged and the tags replaced.
    pub fn tag_copies(&self, tag: &str, picks: &[CopyPick], edit: impl Fn(&mut Purchase)) -> Result<(i64, Vec<String>)> {
        let prefix = SYSTEM_PREFIXES.iter().find(|p| tag.starts_with(**p)).copied();
        let rival = |t: &String| prefix.is_some_and(|p| t.starts_with(p)) && !same_tag(t, tag);
        let what = if prefix == Some(LOT_PREFIX) { "could join the lot" } else { "could take the tag" };
        self.file.mutate(|data| {
            let (mut moved, mut replaced) = (0, Vec::<String>::new());
            for pick in picks {
                let Some(card) = data.cards.get_mut(&pick.key) else { bail!("that card isn't in the collection") };
                let available: i64 = card.copies.each().filter(|(_, _, p)| !has_tag(p, tag)).map(|(_, _, p)| p.quantity).sum();
                let mut want = pick.quantity.unwrap_or(available);
                if want <= 0 || want > available {
                    bail!("{} has {} {} that {what}", card.name, available, if available == 1 { "copy" } else { "copies" });
                }
                // Copies carrying no tag with the same prefix first.
                let mut rows = card.purchase_rows();
                rows.sort_by_key(|r| r.purchase.tags.iter().any(rival));
                let mut copies = super::Copies::default();
                for r in rows {
                    let p = r.purchase;
                    let take = if has_tag(&p, tag) { 0 } else { want.min(p.quantity) };
                    if take > 0 {
                        for t in p.tags.iter().filter(|t| rival(t)) {
                            if !replaced.contains(t) {
                                replaced.push(t.clone());
                            }
                        }
                        let mut tags: Vec<String> = p.tags.iter().filter(|t| !rival(t)).cloned().collect();
                        tags.push(tag.to_string());
                        let mut joined = Purchase { quantity: take, tags: normalize_tags(&tags)?, ..p.clone() };
                        edit(&mut joined);
                        copies.add(&r.finish, r.condition, joined);
                        want -= take;
                        moved += take;
                    }
                    copies.add(&r.finish, r.condition, Purchase { quantity: p.quantity - take, ..p });
                }
                card.copies = copies;
            }
            Ok((moved, replaced))
        })
    }
}

/// Whether a purchase carries `tag` (ignoring case).
fn has_tag(p: &Purchase, tag: &str) -> bool {
    p.tags.iter().any(|t| same_tag(t, tag))
}

/// The purchase with `tag` taken off.
fn without_tag(p: &Purchase, tag: &str) -> Purchase {
    Purchase { tags: p.tags.iter().filter(|t| !same_tag(t, tag)).cloned().collect(), ..p.clone() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::store::tests::{card, tagged, tags_of, v};
    use crate::collection::tags::{lot_of, user_tags};
    use crate::collection::{BulkChange, Condition};

    #[test]
    fn tags_keep_one_spelling() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(dir.path().join("c.json"), "mtg").unwrap();
        s.add_cards(vec![tagged("a", 1, &["Binder A"])]).unwrap();
        s.add_cards(vec![tagged("b", 1, &["  binder   a", "Deck"]), tagged("c", 1, &["deck"])]).unwrap();
        assert_eq!(tags_of(&s, "b"), [(1, v(&["Binder A", "Deck"]))], "the collection's spelling; cleaned");
        assert_eq!(tags_of(&s, "c"), [(1, v(&["Deck"]))], "one spelling within one save too");
        assert_eq!(s.tag_names(), ["Binder A", "Deck"]);
        assert_eq!(s.canonical_tags(v(&["DECK", "new"])), ["Deck", "new"]);
        assert!(s.add_cards(vec![tagged("d", 1, &["a,b"])]).is_err(), "invalid tags are refused");

        // Saving a card takes the rest of the collection's spelling...
        let mut c = s.get("c").unwrap();
        c.copies.0.get_mut("nonfoil").unwrap().get_mut(&Condition::NearMint).unwrap()[0].tags = v(&["binder a"]);
        s.save_card("c", c).unwrap();
        assert_eq!(tags_of(&s, "c"), [(1, v(&["Binder A"]))]);
        // ...but a tag only this card has keeps the spelling it's saved with.
        let mut b = s.get("b").unwrap();
        b.copies.0.get_mut("nonfoil").unwrap().get_mut(&Condition::NearMint).unwrap()[0].tags = v(&["Binder A", "DECK"]);
        s.save_card("b", b).unwrap();
        assert_eq!(tags_of(&s, "b"), [(1, v(&["Binder A", "DECK"]))]);
        // Purchases made identical by a spelling merge.
        s.add_cards(vec![tagged("b", 2, &["binder a", "deck"])]).unwrap();
        assert_eq!(tags_of(&s, "b"), [(3, v(&["Binder A", "DECK"]))]);
    }

    #[test]
    fn rename_and_delete_tags() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(dir.path().join("c.json"), "mtg").unwrap();
        s.add_cards(vec![tagged("a", 1, &["elves"]), tagged("a", 2, &["Binder A"]), tagged("b", 1, &["elves", "keep"])]).unwrap();
        assert_eq!(s.rename_tag("nope", "x").unwrap(), TagEdit::default(), "not in use: nothing, no backup");
        assert!(s.rename_tag("elves", "a;b").is_err());

        let r = s.rename_tag("ELVES", "elf deck").unwrap();
        assert_eq!(r.changed, 2);
        assert!(r.backup.unwrap().ends_with("-tag-rename.json"));
        assert_eq!(s.tag_names(), ["Binder A", "elf deck", "keep"]);

        // Into an existing tag: one tag, the existing spelling; purchases made identical merge.
        assert_eq!(s.rename_tag("elf deck", "binder a").unwrap().changed, 2);
        assert_eq!(tags_of(&s, "a"), [(3, v(&["Binder A"]))]);
        assert_eq!(tags_of(&s, "b"), [(1, v(&["Binder A", "keep"]))]);

        // Case only: the new spelling everywhere.
        s.rename_tag("binder a", "BINDER A").unwrap();
        assert_eq!(s.tag_names(), ["BINDER A", "keep"]);

        let d = s.delete_tag("keep").unwrap();
        assert_eq!(d.changed, 1);
        assert!(d.backup.unwrap().ends_with("-tag-delete.json"));
        assert_eq!(s.tag_names(), ["BINDER A"]);
    }

    #[test]
    fn tags_in_files_merges_lots_and_sales() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("c.json");
        // An old file with no tags loads, and an untagged purchase writes no tags key.
        std::fs::write(&path, r#"{"game":"mtg","cards":{"a":{"id":"a","copies":{"nonfoil":{"NM":[{"quantity":1}]}}}}}"#).unwrap();
        let s = Store::open(&path, "mtg").unwrap();
        assert!(s.get("a").unwrap().purchase_rows()[0].purchase.tags.is_empty());
        s.add_cards(vec![tagged("b", 1, &[])]).unwrap();
        assert!(!std::fs::read_to_string(&path).unwrap().contains("tags"));

        // A conflict copy: purchases with the same tags count once; different tags are kept.
        s.add_cards(vec![tagged("c", 2, &["deck"])]).unwrap();
        s.merge_copy(vec![tagged("c", 1, &["Deck"]), tagged("c", 1, &["binder"])]).unwrap();
        assert_eq!(tags_of(&s, "c"), [(1, v(&["binder"])), (2, v(&["deck"]))]);

        // Taking copies out (a sale) needs the tags to match.
        let p = Purchase { price_paid: 1.0, tags: v(&["deck"]), ..Default::default() };
        assert!(s.take_copies("c", "nonfoil", Condition::NearMint, &Purchase { tags: v(&["other"]), ..p.clone() }, 1).is_err());
        assert_eq!(s.take_copies("c", "nonfoil", Condition::NearMint, &p, 1).unwrap().tags, ["deck"]);

        // Repricing a lot keeps tags.
        let mut in_lot = card("d", 0);
        in_lot.add_purchase("nonfoil", Condition::NearMint, Purchase { quantity: 2, tags: v(&["deck", "lot:L"]), ..Default::default() });
        s.add_cards(vec![in_lot]).unwrap();
        let prices = crate::lots::LotPrices {
            each: [(("d".to_string(), "nonfoil".to_string(), Condition::NearMint), 1.5)].into(),
            ..Default::default()
        };
        s.reprice_lot("L", &prices, "2026-11-15").unwrap();
        let p = &s.get("d").unwrap().purchase_rows()[0].purchase;
        assert_eq!((p.price_paid, user_tags(&p.tags).cloned().collect::<Vec<_>>()), (1.5, v(&["deck"])));
        assert_eq!(lot_of(&p.tags), Some("L"));
    }

    #[test]
    fn lots_reprice_and_drop() {
        use crate::lots::{Lot, LotCount, LotSplit, lot_prices};
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(dir.path().join("c.json"), "mtg").unwrap();
        let in_lot = |id: &str, lot: &str, q: i64| {
            let mut c = card(id, 0);
            c.add_purchase("nonfoil", Condition::NearMint, Purchase { quantity: q, tags: vec![lot_tag(lot)], ..Default::default() });
            c
        };
        s.add_cards(vec![in_lot("a", "L1", 2), in_lot("b", "L1", 1), in_lot("a", "L2", 1), card("c", 1)]).unwrap();
        assert_eq!(s.get("a").unwrap().purchases("nonfoil", Condition::NearMint).len(), 2, "copies from two lots don't merge");

        let lot = Lot {
            id: "L1".into(),
            name: "x".into(),
            total_paid: 10.0,
            split: LotSplit::ByValue,
            count: LotCount::AsEntered,
            ..Lot::default()
        };
        let copies = s.lot_copies("L1", |_| false);
        assert_eq!(copies.iter().map(|c| c.quantity).sum::<i64>(), 3);
        let prices = lot_prices(&lot, &copies);
        assert_eq!(s.reprice_lot("L1", &prices, "2026-11-14").unwrap(), 2);
        let paid: f64 = s
            .cards_with_tag("lot:L1")
            .iter()
            .flat_map(|c| c.purchase_rows())
            .filter(|r| lot_of(&r.purchase.tags) == Some("L1"))
            .map(|r| r.purchase.price_paid * r.purchase.quantity as f64)
            .sum();
        assert!((paid - 10.0).abs() < 1e-9, "{paid}");
        assert!(
            s.get("a")
                .unwrap()
                .purchase_rows()
                .iter()
                .filter(|r| lot_of(&r.purchase.tags) == Some("L1"))
                .all(|r| r.purchase.acquired_date == "2026-11-14")
        );

        // Keep: prices stay, the lot goes; remove: the copies go.
        s.untag_all("lot:L1").unwrap();
        assert!(s.cards_with_tag("lot:L1").is_empty());
        assert_eq!(s.get("b").unwrap().copy_count(), 1);
        s.remove_tagged("lot:L2").unwrap();
        assert_eq!(s.get("a").unwrap().copy_count(), 2);
        assert_eq!(s.len(), 3);
    }

    #[test]
    fn moving_copies_into_and_out_of_lots() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(dir.path().join("c.json"), "mtg").unwrap();
        // a: 3 copies at $1 (no lot) and 2 in lot "old"; b: 1 copy at $1.
        let mut a = card("a", 3);
        a.add_purchase(
            "nonfoil",
            Condition::NearMint,
            Purchase { quantity: 2, price_paid: 0.5, tags: vec![lot_tag("old")], ..Default::default() },
        );
        s.add_cards(vec![a, card("b", 1)]).unwrap();
        let pick = |key: &str, q: Option<i64>| CopyPick { key: key.into(), quantity: q };
        let in_lot = |key: &str, lot: &str| -> i64 {
            s.get(key)
                .map_or(0, |c| c.copies.each().filter(|(_, _, p)| lot_of(&p.tags).unwrap_or("") == lot).map(|(_, _, p)| p.quantity).sum())
        };
        let put_in_lot = |lot: &str, date: &str, picks: &[CopyPick]| -> Result<(i64, Vec<String>)> {
            let (moved, replaced) = s.tag_copies(&lot_tag(lot), picks, |p| p.acquired_date = date.to_string())?;
            Ok((moved, replaced.iter().filter_map(|t| t.strip_prefix(LOT_PREFIX)).map(str::to_string).collect()))
        };

        // Two copies of a: taken from the copies in no lot first.
        assert_eq!(put_in_lot("new", "2026-11-15", &[pick("a", Some(2))]).unwrap(), (2, vec![]));
        assert_eq!((in_lot("a", "new"), in_lot("a", ""), in_lot("a", "old")), (2, 1, 2));
        let joined = s.get("a").unwrap().purchase_rows().into_iter().find(|r| lot_of(&r.purchase.tags) == Some("new")).unwrap().purchase;
        assert_eq!(joined.acquired_date, "2026-11-15", "the lot's date");
        // Every other copy of a (1 in no lot, 2 from "old"), and all of b.
        let (moved, left) = put_in_lot("new", "2026-11-15", &[pick("a", None), pick("b", None)]).unwrap();
        assert_eq!((moved, left), (4, vec!["old".to_string()]));
        assert_eq!((in_lot("a", "new"), in_lot("a", "old"), in_lot("b", "new")), (5, 0, 1));
        assert_eq!(s.get("a").unwrap().copy_count(), 5, "no copies made or lost");
        assert!(put_in_lot("new", "", &[pick("a", None)]).is_err(), "nothing left to join");
        assert!(put_in_lot("new", "", &[pick("b", Some(2))]).is_err());
        assert!(put_in_lot("new", "", &[pick("nope", None)]).is_err());

        // Out of the lot: the copies stay, at the price the lot gave them.
        let mut priced = s.get("b").unwrap();
        priced.copies.0.get_mut("nonfoil").unwrap().get_mut(&Condition::NearMint).unwrap()[0].price_paid = 0.37;
        s.save_card("b", priced).unwrap();
        assert_eq!(s.untag_card("lot:new", "b").unwrap(), 1);
        let p = s.get("b").unwrap().purchase_rows()[0].purchase.clone();
        assert_eq!((lot_of(&p.tags).unwrap_or(""), p.price_paid, p.quantity), ("", 0.37, 1));
        assert!(s.untag_card("lot:new", "b").is_err(), "not in the lot any more");
    }

    #[test]
    fn lots_are_tags() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("c.json");
        let s = Store::open(&path, "mtg").unwrap();
        let mut a = card("a", 0);
        a.add_purchase("nonfoil", Condition::NearMint, Purchase { quantity: 2, tags: v(&["deck", "lot:L1"]), ..Default::default() });
        s.add_cards(vec![a]).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"lot:L1\"") && !text.contains("\"lot\":"), "{text}");
        assert_eq!(s.tag_names(), ["deck"], "the lot's tag isn't listed");
        assert!(s.rename_tag("lot:L1", "x").is_err() && s.delete_tag("lot:L1").is_err());
        assert!(s.rename_tag("deck", "lot:L2").unwrap_err().to_string().contains("can't start with 'lot:'"));

        // Clearing the user's tags leaves the card in its lot.
        let clear = BulkChange { clear_tags: true, ..BulkChange::default() };
        s.bulk_edit(&["a".to_string()], &clear).unwrap();
        let p = s.get("a").unwrap().purchase_rows()[0].purchase.clone();
        assert_eq!(p.tags, ["lot:L1"]);
        for bad in [
            BulkChange { add_tags: v(&["lot:L2"]), ..BulkChange::default() },
            BulkChange { remove_tags: v(&["lot:L1"]), ..BulkChange::default() },
        ] {
            assert!(s.bulk_edit(&["a".to_string()], &bad).is_err(), "{bad:?}");
        }
        let mut two = s.get("a").unwrap();
        two.copies.0.get_mut("nonfoil").unwrap().get_mut(&Condition::NearMint).unwrap()[0].tags = v(&["lot:L1", "lot:L2"]);
        assert!(s.save_card("a", two).unwrap_err().to_string().contains("one lot"));

        // A tag that isn't a lot's: tag_copies and untag_card work on it too.
        s.add_cards(vec![card("b", 2)]).unwrap();
        assert_eq!(s.tag_copies("binder", &[CopyPick { key: "b".into(), quantity: Some(1) }], |_| {}).unwrap(), (1, vec![]));
        assert_eq!(s.cards_with_tag("binder").len(), 1);
        assert_eq!(s.untag_card("binder", "b").unwrap(), 1);
        assert!(s.untag_card("binder", "b").unwrap_err().to_string().contains("doesn't have the tag"));
    }
}
