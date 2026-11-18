//! Tags: the owner's free-text labels on purchases ("binder A", "elves deck"). There
//! is no list of tags to keep: the tags that exist are the ones in use. Tags compare
//! ignoring case, and the first spelling in the collection wins.
//!
//! The app writes tags of its own too, which start with a system prefix: `lot:<id>`
//! puts a purchase in a lot. Users can't type them ([`clean_user_tag`]), and every
//! list or count of the user's tags leaves them out ([`user_tags`]).

use std::collections::BTreeMap;

use anyhow::{Result, bail};
use serde::Serialize;

use super::Card;

/// The longest tag, in characters, and the most tags the user can put on one purchase.
pub const MAX_TAG_LEN: usize = 40;
pub const MAX_TAGS: usize = 20;

/// Tags the app writes itself start with one of these. Users can't type them, and
/// they are left out of everything that lists or counts the user's tags.
pub const LOT_PREFIX: &str = "lot:";
pub const SYSTEM_PREFIXES: &[&str] = &[LOT_PREFIX];

/// Whether the app wrote this tag (it starts with a system prefix).
pub fn is_system_tag(tag: &str) -> bool {
    SYSTEM_PREFIXES.iter().any(|p| tag.starts_with(p))
}

/// The tag that puts a purchase in a lot.
pub fn lot_tag(lot_id: &str) -> String {
    format!("{LOT_PREFIX}{lot_id}")
}

/// The lot a purchase's tags put it in, if any.
pub fn lot_of(tags: &[String]) -> Option<&str> {
    tags.iter().find_map(|t| t.strip_prefix(LOT_PREFIX))
}

/// The tags the user chose: everything but system tags.
pub fn user_tags(tags: &[String]) -> impl Iterator<Item = &String> {
    tags.iter().filter(|t| !is_system_tag(t))
}

/// Whether any of a card's purchases carries `tag` (ignoring case).
pub fn card_has_tag(c: &Card, tag: &str) -> bool {
    c.copies.each().any(|(_, _, p)| p.tags.iter().any(|t| same_tag(t, tag)))
}

/// Why a tag can't be used, or None if it can.
fn tag_problem(raw: &str) -> Option<&'static str> {
    let clean = tidy(raw);
    if raw.chars().any(char::is_control) {
        Some("a tag can't contain control characters")
    } else if clean.is_empty() {
        Some("a tag can't be empty")
    } else if clean.chars().count() > MAX_TAG_LEN {
        Some("a tag can be at most 40 characters")
    } else if clean.contains([',', ';']) {
        Some("a tag can't contain a comma or a semicolon")
    } else {
        None
    }
}

/// Trims and collapses runs of whitespace to one space.
fn tidy(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Cleans one tag: trims it and collapses runs of whitespace to one space. None if
/// it is empty, longer than MAX_TAG_LEN, or contains a comma, a semicolon or a
/// control character (commas and semicolons separate tags in text fields and CSV).
pub fn clean_tag(raw: &str) -> Option<String> {
    tag_problem(raw).is_none().then(|| tidy(raw))
}

/// Cleans a tag typed by the user, as [`clean_tag`] does, and refuses one starting
/// with a system prefix. Every place tags arrive from outside (a saved purchase, the
/// add form, bulk add and remove, an import's Tags column, a rename's new name)
/// checks them with this.
pub fn clean_user_tag(raw: &str) -> Result<String> {
    if let Some(p) = SYSTEM_PREFIXES.iter().find(|p| raw.trim().starts_with(**p)) {
        bail!("tags can't start with '{p}'");
    }
    match clean_tag(raw) {
        Some(t) => Ok(t),
        None => bail!("{}: {:?}", tag_problem(raw).unwrap_or("that tag can't be used"), raw.trim()),
    }
}

/// Checks tags typed by the user with [`clean_user_tag`], then normalizes them.
pub fn normalize_user_tags(raw: &[String]) -> Result<Vec<String>> {
    for t in raw {
        clean_user_tag(t)?;
    }
    normalize_tags(raw)
}

/// Cleans every tag, drops duplicates ignoring case (the first spelling stays), and
/// sorts ignoring case. Errors if a tag is invalid, if there are more than MAX_TAGS
/// user tags, or if the tags put the purchase in more than one lot. System tags are
/// accepted, so the app can write them.
pub fn normalize_tags(raw: &[String]) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for t in raw {
        let Some(clean) = clean_tag(t) else {
            bail!("{}: {:?}", tag_problem(t).unwrap_or("that tag can't be used"), t.trim());
        };
        if !out.iter().any(|o| same_tag(o, &clean)) {
            out.push(clean);
        }
    }
    if user_tags(&out).count() > MAX_TAGS {
        bail!("at most {MAX_TAGS} tags on one purchase");
    }
    if out.iter().filter(|t| t.starts_with(LOT_PREFIX)).count() > 1 {
        bail!("a purchase can be in one lot at most");
    }
    sort_tags(&mut out);
    Ok(out)
}

/// Sorts tags ignoring case (then by spelling, so the order is total).
pub fn sort_tags(tags: &mut [String]) {
    tags.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()).then_with(|| a.cmp(b)));
}

/// Splits "binder A; elves deck" or "binder A, elves deck" into tags (not yet
/// normalized).
pub fn split_tags(text: &str) -> Vec<String> {
    text.split([',', ';']).map(str::trim).filter(|t| !t.is_empty()).map(str::to_string).collect()
}

/// Whether two tags are the same, ignoring case.
pub fn same_tag(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

/// Whether two normalized tag lists are the same, ignoring case.
pub fn same_tags(a: &[String], b: &[String]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| same_tag(x, y))
}

/// Every user tag in use on `cards` (purchases with copies), sorted ignoring case,
/// each in its first spelling met.
pub fn tags_in_use<'a>(cards: impl IntoIterator<Item = &'a Card>) -> Vec<String> {
    let mut by_key: BTreeMap<String, String> = BTreeMap::new();
    for c in cards {
        for (_, _, p) in c.copies.each() {
            for t in user_tags(&p.tags) {
                by_key.entry(t.to_lowercase()).or_insert_with(|| t.clone());
            }
        }
    }
    let mut out: Vec<String> = by_key.into_values().collect();
    sort_tags(&mut out);
    out
}

/// Replaces each tag with the spelling in `existing` that matches it ignoring case,
/// if there is one.
pub fn with_spellings(tags: Vec<String>, existing: &[String]) -> Vec<String> {
    let mut out: Vec<String> = tags.into_iter().map(|t| existing.iter().find(|e| same_tag(e, &t)).cloned().unwrap_or(t)).collect();
    sort_tags(&mut out);
    out
}

/// A card's user tags: the union across its purchases with copies, sorted ignoring
/// case.
pub fn card_tags(c: &Card) -> Vec<String> {
    tags_in_use(std::iter::once(c))
}

/// One tag's totals over the purchases that carry it.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct TagSummary {
    pub name: String,
    pub cards: usize,
    pub copies: i64,
    pub value: f64,
    pub paid: f64,
}

/// Every tag's totals, and the untagged copies' (None when every copy is tagged).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct TagSummaries {
    pub tags: Vec<TagSummary>,
    pub untagged: Option<TagSummary>,
}

/// Every user tag in use, with its totals, sorted by name ignoring case. `cards`
/// counts printings with at least one tagged purchase; `value` uses each purchase's
/// own finish price; `paid` sums price paid over the tagged copies. A purchase with no
/// user tags (only system ones, or none) is untagged.
pub fn tag_summaries(cards: &[Card]) -> TagSummaries {
    #[derive(Default)]
    struct Tally {
        name: String,
        ids: std::collections::HashSet<String>,
        copies: i64,
        value: f64,
        paid: f64,
    }
    impl Tally {
        fn add(&mut self, id: &str, copies: i64, value: f64, paid: f64) {
            self.ids.insert(id.to_string());
            self.copies += copies;
            self.value += value;
            self.paid += paid;
        }
        fn summary(self) -> TagSummary {
            let round = super::round_cents;
            TagSummary { name: self.name, cards: self.ids.len(), copies: self.copies, value: round(self.value), paid: round(self.paid) }
        }
    }
    let mut by_key: BTreeMap<String, Tally> = BTreeMap::new();
    let mut untagged = Tally { name: "Untagged".into(), ..Tally::default() };
    for c in cards {
        for (f, _, p) in c.copies.each() {
            let value = c.market.get(f).copied().unwrap_or(0.0) * p.quantity as f64;
            let paid = p.price_paid * p.quantity as f64;
            if user_tags(&p.tags).next().is_none() {
                untagged.add(&c.id, p.quantity, value, paid);
            }
            for t in user_tags(&p.tags) {
                let tally = by_key.entry(t.to_lowercase()).or_insert_with(|| Tally { name: t.clone(), ..Tally::default() });
                tally.add(&c.id, p.quantity, value, paid);
            }
        }
    }
    let mut tags: Vec<TagSummary> = by_key.into_values().map(Tally::summary).collect();
    tags.sort_by_key(|t| t.name.to_lowercase());
    TagSummaries { tags, untagged: (untagged.copies > 0).then(|| untagged.summary()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::{Condition, Purchase};

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn cleaning_and_normalizing() {
        assert_eq!(clean_tag("  binder   A "), Some("binder A".into()));
        for bad in ["", "   ", "a,b", "a;b", "bell\u{7}", "tab\there", "line\nbreak", &"x".repeat(41)] {
            assert_eq!(clean_tag(bad), None, "{bad:?}");
        }
        assert_eq!(clean_tag(&"é".repeat(40)), Some("é".repeat(40)), "40 characters, not bytes");

        let got = normalize_tags(&strings(&["elves deck", " Binder  A", "binder a", "ZZ", "apple"])).unwrap();
        assert_eq!(got, ["apple", "Binder A", "elves deck", "ZZ"], "first spelling stays; sorted ignoring case");
        assert!(normalize_tags(&[]).unwrap().is_empty());
        for (bad, why) in [("a,b", "comma"), (" ", "empty"), (&"x".repeat(41)[..], "40 characters"), ("a\u{7}", "control")] {
            let err = normalize_tags(&strings(&[bad])).unwrap_err().to_string();
            assert!(err.contains(why), "{bad:?}: {err}");
        }
        let twenty: Vec<String> = (0..20).map(|i| format!("t{i}")).collect();
        assert_eq!(normalize_tags(&twenty).unwrap().len(), 20);
        let mut too_many = twenty.clone();
        too_many.push("t20".into());
        assert!(normalize_tags(&too_many).unwrap_err().to_string().contains("at most 20"));
        too_many.pop();
        too_many.push("T0".into());
        assert_eq!(normalize_tags(&too_many).unwrap().len(), 20, "a duplicate isn't one more");
    }

    #[test]
    fn splitting_and_comparing() {
        assert_eq!(split_tags("binder A; elves deck, ,for sale;"), ["binder A", "elves deck", "for sale"]);
        assert!(split_tags("").is_empty());
        assert!(same_tag("Binder A", "binder a") && !same_tag("binder", "binder a"));
        assert_eq!(with_spellings(strings(&["binder a", "new"]), &strings(&["Binder A"])), ["Binder A", "new"]);
    }

    #[test]
    fn system_tags() {
        assert_eq!(lot_tag("ab12"), "lot:ab12");
        assert!(is_system_tag("lot:ab12") && !is_system_tag("lots of elves"));
        let tags = strings(&["binder A", "lot:ab12"]);
        assert_eq!(lot_of(&tags), Some("ab12"));
        assert_eq!(lot_of(&strings(&["binder A"])), None);
        assert_eq!(user_tags(&tags).collect::<Vec<_>>(), ["binder A"]);
        assert_eq!(clean_tag("lot:ab12"), Some("lot:ab12".into()), "the app can write them");
        for bad in ["lot:ab12", "  lot:x"] {
            assert!(clean_user_tag(bad).unwrap_err().to_string().contains("tags can't start with 'lot:'"), "{bad:?}");
        }
        assert_eq!(clean_user_tag(" Binder  A ").unwrap(), "Binder A");
        assert!(normalize_user_tags(&strings(&["ok", "lot:x"])).is_err());
        assert!(normalize_tags(&strings(&["lot:a", "lot:b"])).unwrap_err().to_string().contains("one lot"));
        let mut twenty: Vec<String> = (0..20).map(|i| format!("t{i}")).collect();
        twenty.push(lot_tag("a"));
        assert_eq!(normalize_tags(&twenty).unwrap().len(), 21, "the lot tag isn't one of the 20");
    }

    #[test]
    fn a_card_in_a_lot_is_untagged() {
        let mut c = Card { id: "a".into(), market: [("nonfoil".into(), 2.0)].into(), ..Card::default() };
        c.add_purchase("nonfoil", Condition::NearMint, Purchase { quantity: 3, tags: strings(&["lot:L1"]), ..Default::default() });
        assert!(tags_in_use([&c]).is_empty() && card_tags(&c).is_empty());
        let s = tag_summaries(std::slice::from_ref(&c));
        assert!(s.tags.is_empty());
        assert_eq!(s.untagged.map(|u| u.copies), Some(3));
        assert!(card_has_tag(&c, "lot:L1") && !card_has_tag(&c, "lot:L2"));
    }

    #[test]
    fn summaries_count_only_tagged_copies() {
        let mut c = Card { id: "a".into(), market: [("nonfoil".into(), 2.0), ("foil".into(), 10.0)].into(), ..Card::default() };
        let buy = |q, paid: f64, tags: &[&str]| Purchase { quantity: q, price_paid: paid, tags: strings(tags), ..Default::default() };
        c.add_purchase("nonfoil", Condition::NearMint, buy(4, 1.0, &["elves deck"]));
        c.add_purchase("foil", Condition::NearMint, buy(1, 8.0, &["Binder A", "elves deck"]));
        c.add_purchase("nonfoil", Condition::LightPlay, buy(2, 0.0, &[]));
        let s = tag_summaries(&[c]);
        assert_eq!(
            s.tags,
            [
                TagSummary { name: "Binder A".into(), cards: 1, copies: 1, value: 10.0, paid: 8.0 },
                TagSummary { name: "elves deck".into(), cards: 1, copies: 5, value: 18.0, paid: 12.0 },
            ]
        );
        assert_eq!(s.untagged, Some(TagSummary { name: "Untagged".into(), cards: 1, copies: 2, value: 4.0, paid: 0.0 }));
        assert_eq!(tag_summaries(&[]).untagged, None);
    }
}
