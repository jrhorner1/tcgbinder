//! Narrowing and ordering the collection list: `Query` (every filter the Collection
//! page offers), `SortKey` and `sort_cards`, and the set filter's choices.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use super::tags::{same_tag, user_tags};
use super::{Card, Condition};

/// The "no filter" choice in filter menus.
pub const FILTER_ANY: &str = "Any";

/// Whether a filter value means "no filter".
pub fn is_any(s: &str) -> bool {
    s.is_empty() || s == FILTER_ANY
}

/// An extra, game-specific match on a card.
pub type CardMatch<'a> = Box<dyn Fn(&Card) -> bool + 'a>;

/// How to narrow the collection view. The default matches everything.
#[derive(Default)]
pub struct Query<'a> {
    /// Whitespace-separated terms; every term must match the name, set code, set name,
    /// collector number, or a purchase's note or tags.
    pub text: String,
    /// Set code (case-insensitive); "" or [`FILTER_ANY`] for all.
    pub set: String,
    pub rarity: String,
    /// The finish (case-insensitive); "" or [`FILTER_ANY`] for all.
    pub finish: String,
    /// A condition code ("NM"): any copy in it.
    pub condition: String,
    /// The printing's language code (case-insensitive).
    pub language: String,
    /// Text the type line contains, ignoring case ("creature", "legendary").
    pub card_type: String,
    /// Only cards with a note on a purchase.
    pub has_note: bool,
    /// One copy's price ([`Card::best_price`]) at least / at most this; unpriced cards
    /// don't match either bound.
    pub price_min: Option<f64>,
    pub price_max: Option<f64>,
    /// Any purchase dated on or after / on or before this (`YYYY-MM-DD`); undated
    /// purchases don't match either bound.
    pub acquired_from: String,
    pub acquired_to: String,
    /// At least this many copies.
    pub min_copies: Option<i64>,
    /// Only cards with a copy that has no price paid.
    pub no_price_paid: bool,
    /// Only cards with a copy whose finish has no market price.
    pub unpriced: bool,
    /// Tags the card must have. With `tags_any` false (the default) it must have all
    /// of them; with it true, at least one. A card "has" a tag when any of its
    /// purchases with copies carries it.
    pub tags: Vec<String>,
    pub tags_any: bool,
    /// Tags the card must not have on any purchase.
    pub tags_not: Vec<String>,
    /// Only cards with at least one purchase that has no tags.
    pub untagged: bool,
    /// An extra, game-specific condition (e.g. Magic's color filter).
    pub matches: Option<CardMatch<'a>>,
}

impl Query<'_> {
    /// Whether a card passes the tag filters.
    fn tags_match(&self, c: &Card) -> bool {
        if self.tags.is_empty() && self.tags_not.is_empty() && !self.untagged {
            return true;
        }
        let has = |want: &str| c.copies.each().any(|(_, _, p)| p.tags.iter().any(|t| same_tag(t, want)));
        let wanted =
            self.tags.is_empty() || if self.tags_any { self.tags.iter().any(|t| has(t)) } else { self.tags.iter().all(|t| has(t)) };
        wanted
            && !self.tags_not.iter().any(|t| has(t))
            && (!self.untagged || c.copies.each().any(|(_, _, p)| user_tags(&p.tags).next().is_none()))
    }

    /// Tells whether a card matches the query.
    pub fn matcher(&self) -> impl Fn(&Card) -> bool + '_ {
        let terms: Vec<String> = self.text.to_lowercase().split_whitespace().map(str::to_string).collect();
        let card_type = self.card_type.trim().to_lowercase();
        let condition = (!is_any(&self.condition)).then(|| Condition::parse(&self.condition));
        let dated = !self.acquired_from.is_empty() || !self.acquired_to.is_empty();
        let in_range = |d: &str| {
            !d.is_empty()
                && (self.acquired_from.is_empty() || d >= self.acquired_from.as_str())
                && (self.acquired_to.is_empty() || d <= self.acquired_to.as_str())
        };
        move |c| {
            (is_any(&self.set) || c.set.eq_ignore_ascii_case(&self.set))
                && (is_any(&self.rarity) || c.rarity.eq_ignore_ascii_case(&self.rarity))
                && (is_any(&self.finish) || c.has_finish(&self.finish))
                && (is_any(&self.language) || c.language.eq_ignore_ascii_case(&self.language))
                && (card_type.is_empty() || contains_fold(&c.type_line, &card_type))
                && condition.is_none_or(|want| c.copies.each().any(|(_, cond, p)| cond == want && p.quantity > 0))
                && (!self.has_note || c.copies.each().any(|(_, _, p)| !p.note.trim().is_empty()))
                && (self.price_min.is_none() && self.price_max.is_none()
                    || c.best_price().is_some_and(|x| self.price_min.is_none_or(|m| x >= m) && self.price_max.is_none_or(|m| x <= m)))
                && (!dated || c.copies.each().any(|(_, _, p)| in_range(&p.acquired_date)))
                && self.min_copies.is_none_or(|n| c.copy_count() >= n)
                && (!self.no_price_paid || c.price_paid().1 > 0)
                && (!self.unpriced || c.market_value().1 > 0)
                && self.tags_match(c)
                && self.matches.as_ref().is_none_or(|m| m(c))
                // Each term in the name, set code, set name, number or a purchase's note.
                && terms.iter().all(|t| {
                    [&c.name, &c.set, &c.set_name, &c.collector_number].iter().any(|field| contains_fold(field, t))
                        || c.copies.each().any(|(_, _, p)| contains_fold(&p.note, t) || user_tags(&p.tags).any(|tag| contains_fold(tag, t)))
                })
        }
    }
}

/// Whether `s` contains `sub` (already lowercase), ignoring case, without allocating
/// for ASCII text.
fn contains_fold(s: &str, sub: &str) -> bool {
    if !s.is_ascii() || !sub.is_ascii() {
        return s.to_lowercase().contains(sub);
    }
    let (s, sub) = (s.as_bytes(), sub.as_bytes());
    sub.is_empty() || s.windows(sub.len()).any(|w| w.iter().zip(sub).all(|(a, b)| a.to_ascii_lowercase() == *b))
}

/// Compares strings case-insensitively, without allocating for ASCII text.
fn compare_fold(a: &str, b: &str) -> Ordering {
    if a.is_ascii() && b.is_ascii() {
        a.bytes().map(|c| c.to_ascii_lowercase()).cmp(b.bytes().map(|c| c.to_ascii_lowercase()))
    } else {
        a.to_lowercase().cmp(&b.to_lowercase())
    }
}

/// The tiebreaker: name, then set, then collector number, then ID for determinism.
pub fn default_order(a: &Card, b: &Card) -> Ordering {
    compare_fold(&a.name, &b.name)
        .then_with(|| compare_fold(&a.set, &b.set))
        .then_with(|| compare_collector_number(&a.collector_number, &b.collector_number))
        .then_with(|| a.id.cmp(&b.id))
}

/// Orders "2" < "10" < "10a" < "A1": the leading numeric part numerically, then the
/// remainder.
pub fn compare_collector_number(a: &str, b: &str) -> Ordering {
    let (an, ar) = split_number(a);
    let (bn, br) = split_number(b);
    match (an, bn) {
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        _ => an.cmp(&bn).then_with(|| ar.to_lowercase().cmp(&br.to_lowercase())),
    }
}

fn split_number(s: &str) -> (Option<u64>, &str) {
    let digits = s.bytes().take_while(u8::is_ascii_digit).count();
    match s[..digits].parse() {
        Ok(n) => (Some(n), &s[digits..]),
        Err(_) => (None, s),
    }
}

/// A set filter choice: the code, and a label like "M19 — Core Set 2019".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetOption {
    pub code: String,
    pub label: String,
}

/// The distinct sets in `cards`, sorted by set name.
pub fn set_options(cards: &[Card]) -> Vec<SetOption> {
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    for c in cards.iter().filter(|c| !c.set.is_empty()) {
        seen.entry(c.set.to_uppercase()).or_insert_with(|| c.set_name.clone());
    }
    let mut out: Vec<SetOption> = seen
        .into_iter()
        .map(|(code, name)| {
            let label = if name.is_empty() { code.clone() } else { format!("{code} — {name}") };
            SetOption { code, label }
        })
        .collect();
    let name = |o: &SetOption| o.label.split_once(" — ").map_or(String::new(), |(_, n)| n.to_lowercase());
    out.sort_by_key(name);
    out
}

/// What the collection list can be sorted by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortKey {
    Name,
    /// Set release date, then set code, then collector number.
    Set,
    Number,
    /// One copy's price ([`Card::best_price`]); unpriced counts as 0.
    Price,
    /// Every copy's market value.
    Value,
    /// Value minus price paid; cards without any price paid come last.
    Gain,
    /// The total price paid.
    Paid,
    Quantity,
    /// The earliest purchase date; undated cards come last.
    Acquired,
    Released,
    /// The game's rarity order, lowest first; unknown rarities after.
    Rarity,
    /// The card's first tag alphabetically (ignoring case); untagged cards last.
    Tag,
}

impl SortKey {
    pub const ALL: &[(&str, SortKey)] = &[
        ("name", SortKey::Name),
        ("set", SortKey::Set),
        ("number", SortKey::Number),
        ("price", SortKey::Price),
        ("value", SortKey::Value),
        ("gain", SortKey::Gain),
        ("paid", SortKey::Paid),
        ("quantity", SortKey::Quantity),
        ("acquired", SortKey::Acquired),
        ("released", SortKey::Released),
        ("rarity", SortKey::Rarity),
        ("tag", SortKey::Tag),
    ];

    pub fn parse(s: &str) -> Option<SortKey> {
        Self::ALL.iter().find(|(name, _)| name.eq_ignore_ascii_case(s)).map(|(_, k)| *k)
    }

    /// The key and direction (descending?) for a Card order setting: "newest" is set
    /// descending, "oldest" set ascending, "value" value descending, anything else name.
    pub fn from_card_order(order: &str) -> (SortKey, bool) {
        match order {
            "newest" => (SortKey::Set, true),
            "oldest" => (SortKey::Set, false),
            "value" => (SortKey::Value, true),
            _ => (SortKey::Name, false),
        }
    }
}

/// A card's first tag alphabetically, lower-cased (for sorting).
fn first_tag(c: &Card) -> Option<String> {
    c.copies.each().flat_map(|(_, _, p)| user_tags(&p.tags)).map(|t| t.to_lowercase()).min()
}

/// The earliest dated purchase.
fn first_acquired(c: &Card) -> Option<&str> {
    c.copies.each().map(|(_, _, p)| p.acquired_date.as_str()).filter(|d| !d.is_empty()).min()
}

/// Sorts by `key`, descending if asked. Only the key itself is reversed: cards that
/// tie on it stay in [`default_order`] (for `Set`, only the release date is reversed,
/// so a set's cards stay in number order), and cards with nothing to sort by
/// (undated for `Acquired`, no price paid for `Gain`, untagged for `Tag`) come last
/// either way.
/// `rarity_order` is the game's rarities, lowest first.
pub fn sort_cards(mut cards: Vec<Card>, key: SortKey, descending: bool, rarity_order: &[&str]) -> Vec<Card> {
    let dir = |o: Ordering| if descending { o.reverse() } else { o };
    // Missing values last, whichever the direction.
    let last = |a: Option<Ordering>, a_has: bool, b_has: bool| match (a_has, b_has) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => a.map_or(Ordering::Equal, dir),
    };
    let rank = |c: &Card| rarity_order.iter().position(|r| r.eq_ignore_ascii_case(&c.rarity)).unwrap_or(rarity_order.len());
    let gain = |c: &Card| {
        let (paid, unknown) = c.price_paid();
        (unknown < c.copy_count()).then(|| c.market_value().0 - paid)
    };
    cards.sort_by(|a, b| {
        let primary = match key {
            SortKey::Name => dir(compare_fold(&a.name, &b.name)),
            SortKey::Set => dir(a.released_at.cmp(&b.released_at))
                .then_with(|| compare_fold(&a.set, &b.set))
                .then_with(|| compare_collector_number(&a.collector_number, &b.collector_number)),
            SortKey::Number => dir(compare_collector_number(&a.collector_number, &b.collector_number)),
            SortKey::Price => dir(a.best_price().unwrap_or(0.0).total_cmp(&b.best_price().unwrap_or(0.0))),
            SortKey::Value => dir(a.market_value().0.total_cmp(&b.market_value().0)),
            SortKey::Paid => dir(a.price_paid().0.total_cmp(&b.price_paid().0)),
            SortKey::Quantity => dir(a.copy_count().cmp(&b.copy_count())),
            SortKey::Released => dir(a.released_at.cmp(&b.released_at)),
            SortKey::Rarity => dir(rank(a).cmp(&rank(b))),
            SortKey::Gain => {
                let (x, y) = (gain(a), gain(b));
                last(x.zip(y).map(|(x, y)| x.total_cmp(&y)), x.is_some(), y.is_some())
            }
            SortKey::Acquired => {
                let (x, y) = (first_acquired(a), first_acquired(b));
                last(x.zip(y).map(|(x, y)| x.cmp(y)), x.is_some(), y.is_some())
            }
            SortKey::Tag => {
                let (x, y) = (first_tag(a), first_tag(b));
                last(x.as_ref().zip(y.as_ref()).map(|(x, y)| x.cmp(y)), x.is_some(), y.is_some())
            }
        };
        primary.then_with(|| default_order(a, b))
    });
    cards
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::{Condition, Purchase};

    fn card(name: &str, set: &str, number: &str, rarity: &str, finish: &str) -> Card {
        let mut c = Card {
            id: format!("{set}-{number}"),
            name: name.into(),
            set: set.into(),
            set_name: format!("{set} Edition"),
            collector_number: number.into(),
            rarity: rarity.into(),
            ..Card::default()
        };
        c.add_purchase(finish, Condition::NearMint, Purchase { quantity: 1, ..Default::default() });
        c
    }

    fn by_order(cards: Vec<Card>, order: &str) -> Vec<Card> {
        let (key, desc) = SortKey::from_card_order(order);
        sort_cards(cards, key, desc, &[])
    }

    fn names(cards: &[Card]) -> Vec<&str> {
        cards.iter().map(|c| c.name.as_str()).collect()
    }

    #[test]
    fn filters_and_terms() {
        let cards = [
            card("Lightning Bolt", "M11", "149", "common", "nonfoil"),
            card("Shock", "M19", "156", "common", "foil"),
            card("Scapeshift", "M19", "201", "rare", "nonfoil"),
            card("Éclair", "ABC", "1", "rare", "nonfoil"),
        ];
        let q = |f: &dyn Fn(&mut Query)| {
            let mut q = Query::default();
            f(&mut q);
            let matched = cards.iter().filter(|c| q.matcher()(c)).cloned().collect();
            names(&sort_cards(matched, SortKey::Name, false, &[])).join(",")
        };
        assert_eq!(q(&|_| {}), "Lightning Bolt,Scapeshift,Shock,Éclair");
        assert_eq!(q(&|q| q.set = "m19".into()), "Scapeshift,Shock");
        assert_eq!(q(&|q| q.rarity = "RARE".into()), "Scapeshift,Éclair");
        assert_eq!(q(&|q| q.finish = "Foil".into()), "Shock");
        assert_eq!(q(&|q| q.text = "m19 sh".into()), "Scapeshift,Shock");
        assert_eq!(q(&|q| q.text = "éCL".into()), "Éclair");
        assert_eq!(q(&|q| q.text = "149".into()), "Lightning Bolt");
        assert_eq!(q(&|q| q.set = FILTER_ANY.into()), "Lightning Bolt,Scapeshift,Shock,Éclair");
        assert_eq!(q(&|q| q.matches = Some(Box::new(|c: &Card| c.name.len() == 5))), "Shock");
    }

    #[test]
    fn search_finds_notes() {
        let mut noted = card("Opt", "DOM", "60", "common", "nonfoil");
        noted.add_purchase(
            "foil",
            Condition::NearMint,
            Purchase { quantity: 1, note: "Trade binder, page 3".into(), ..Default::default() },
        );
        let plain = card("Shock", "M19", "156", "common", "nonfoil");
        let find = |text: &str| {
            let q = Query { text: text.into(), ..Query::default() };
            [&noted, &plain].iter().filter(|c| q.matcher()(c)).map(|c| c.name.clone()).collect::<Vec<_>>()
        };
        assert_eq!(find("binder"), ["Opt"]);
        assert_eq!(find("opt page"), ["Opt"], "terms can mix name and note");
        assert!(find("binder shock").is_empty());
    }

    #[test]
    fn collector_numbers() {
        let mut v = vec!["10a", "A1", "2", "10", "★1"];
        v.sort_by(|a, b| compare_collector_number(a, b));
        assert_eq!(v, ["2", "10", "10a", "A1", "★1"]);
    }

    #[test]
    fn options_and_orders() {
        let mut a = card("Bolt", "M11", "149", "common", "nonfoil");
        a.released_at = "2010-07-16".into();
        a.market.insert("nonfoil".into(), 1.0);
        let mut b = card("Shock", "M19", "156", "common", "nonfoil");
        b.released_at = "2018-07-13".into();
        b.set_name = "Core Set 2019".into();
        b.market.insert("nonfoil".into(), 0.1);
        let opts = set_options(&[a.clone(), b.clone()]);
        assert_eq!(opts[0].label, "M19 — Core Set 2019");
        assert_eq!(opts[1].code, "M11");
        let cards = vec![a, b];
        assert_eq!(names(&by_order(cards.clone(), "newest")), ["Shock", "Bolt"]);
        assert_eq!(names(&by_order(cards.clone(), "oldest")), ["Bolt", "Shock"]);
        assert_eq!(names(&by_order(cards.clone(), "value")), ["Bolt", "Shock"]);
        assert_eq!(names(&by_order(cards, "name")), ["Bolt", "Shock"]);
    }

    /// Four cards that differ on every sort key.
    fn sortable() -> Vec<Card> {
        let make = |name: &str, set: &str, number: &str, released: &str, rarity: &str| {
            let mut c = Card {
                id: format!("{set}-{number}"),
                name: name.into(),
                set: set.into(),
                collector_number: number.into(),
                released_at: released.into(),
                rarity: rarity.into(),
                ..Card::default()
            };
            c.market.insert("nonfoil".into(), 0.0);
            c
        };
        let buy = |q, price, date: &str| Purchase { quantity: q, price_paid: price, acquired_date: date.into(), ..Default::default() };
        // Bolt: 2 copies at $1.00 each, paid $0.50 each, 2020-05-01. Value 2, gain 1.
        let mut bolt = make("Bolt", "M11", "149", "2010-07-16", "common");
        bolt.market.insert("nonfoil".into(), 1.0);
        bolt.add_purchase("nonfoil", Condition::NearMint, buy(2, 0.5, "2020-05-01"));
        // Opal: 1 copy at $9.00, paid $10.00, 2019-01-01. Value 9, gain -1.
        let mut opal = make("Opal", "USG", "10", "1998-10-12", "rare");
        opal.market.insert("nonfoil".into(), 9.0);
        opal.add_purchase("nonfoil", Condition::NearMint, buy(1, 10.0, "2019-01-01"));
        // Shock: 4 copies at $0.25, no price paid, undated. Value 1, no gain.
        let mut shock = make("Shock", "M19", "2", "2018-07-13", "uncommon");
        shock.market.insert("nonfoil".into(), 0.25);
        shock.add_purchase("nonfoil", Condition::NearMint, buy(4, 0.0, ""));
        // Zap: 3 copies at $0.50, paid $0.10 each, 2021-03-03, rarity unknown. Value 1.5, gain 1.2.
        let mut zap = make("Zap", "M19", "10", "2018-07-13", "special");
        zap.market.insert("nonfoil".into(), 0.5);
        zap.add_purchase("nonfoil", Condition::NearMint, buy(3, 0.1, "2021-03-03"));
        // Tags: Bolt "zoo", Shock "burn", Zap "Alpha"; Opal none.
        tag(&mut bolt, &["zoo"]);
        tag(&mut shock, &["burn"]);
        tag(&mut zap, &["Alpha"]);
        vec![bolt, opal, shock, zap]
    }

    /// Puts `tags` on every purchase of `c`.
    fn tag(c: &mut Card, tags: &[&str]) {
        for list in c.copies.0.values_mut().flat_map(|m| m.values_mut()) {
            for p in list {
                p.tags = tags.iter().map(|t| t.to_string()).collect();
            }
        }
    }

    #[test]
    fn every_sort_key_both_ways() {
        let rarities = ["common", "uncommon", "rare", "mythic"];
        let sorted = |key: &str, desc: bool| {
            let key = SortKey::parse(key).unwrap_or_else(|| panic!("{key}"));
            names(&sort_cards(sortable(), key, desc, &rarities)).join(",")
        };
        let cases = [
            ("name", "Bolt,Opal,Shock,Zap", "Zap,Shock,Opal,Bolt"),
            // Set: release date, then code, then number; reversing keeps a set's numbers in order.
            ("set", "Opal,Bolt,Shock,Zap", "Shock,Zap,Bolt,Opal"),
            ("number", "Shock,Opal,Zap,Bolt", "Bolt,Opal,Zap,Shock"),
            ("price", "Shock,Zap,Bolt,Opal", "Opal,Bolt,Zap,Shock"),
            ("value", "Shock,Zap,Bolt,Opal", "Opal,Bolt,Zap,Shock"),
            // Shock has no price paid: last both ways.
            ("gain", "Opal,Bolt,Zap,Shock", "Zap,Bolt,Opal,Shock"),
            ("paid", "Shock,Zap,Bolt,Opal", "Opal,Bolt,Zap,Shock"),
            ("quantity", "Opal,Bolt,Zap,Shock", "Shock,Zap,Bolt,Opal"),
            // Shock is undated: last both ways.
            ("acquired", "Opal,Bolt,Zap,Shock", "Zap,Bolt,Opal,Shock"),
            // Shock and Zap tie on release date: by name both ways.
            ("released", "Opal,Bolt,Shock,Zap", "Shock,Zap,Bolt,Opal"),
            // Zap's rarity isn't in the game's order: after the known ones.
            ("rarity", "Bolt,Shock,Opal,Zap", "Zap,Opal,Shock,Bolt"),
            // First tag ignoring case: Alpha, burn, zoo; Opal is untagged: last both ways.
            ("tag", "Zap,Shock,Bolt,Opal", "Bolt,Shock,Zap,Opal"),
        ];
        assert_eq!(cases.len(), SortKey::ALL.len());
        for (key, asc, desc) in cases {
            assert_eq!(sorted(key, false), asc, "{key} ascending");
            assert_eq!(sorted(key, true), desc, "{key} descending");
        }
        assert_eq!(SortKey::parse("VALUE"), Some(SortKey::Value));
        assert_eq!(SortKey::parse("nope"), None);
    }

    #[test]
    fn more_filters() {
        let mut cards = sortable();
        cards[0].type_line = "Instant".into();
        cards[1].type_line = "Artifact".into();
        cards[2].type_line = "Instant".into();
        cards[3].type_line = "Legendary Creature — Goblin".into();
        cards[1].language = "ja".into();
        for c in &mut cards[..1] {
            c.add_purchase("nonfoil", Condition::LightPlay, Purchase { quantity: 1, note: " ".into(), ..Default::default() });
        }
        cards[3].add_purchase(
            "foil",
            Condition::NearMint,
            Purchase { quantity: 1, note: "binder".into(), tags: vec!["lot:lot1".into()], ..Default::default() },
        );
        cards[2].market.clear(); // Shock: unpriced
        let q = |f: &dyn Fn(&mut Query)| {
            let mut q = Query::default();
            f(&mut q);
            let m = q.matcher();
            names(&cards.iter().filter(|c| m(c)).cloned().collect::<Vec<_>>()).join(",")
        };
        assert_eq!(q(&|q| q.condition = "LP".into()), "Bolt");
        assert_eq!(q(&|q| q.condition = FILTER_ANY.into()), "Bolt,Opal,Shock,Zap");
        assert_eq!(q(&|q| q.language = "JA".into()), "Opal");
        assert_eq!(q(&|q| q.card_type = " creature ".into()), "Zap");
        assert_eq!(q(&|q| q.card_type = "instant".into()), "Bolt,Shock");
        assert_eq!(q(&|q| q.tags = vec!["lot:lot1".into()]), "Zap");
        assert_eq!(q(&|q| q.text = "lot1".into()), "", "the lot's tag isn't searched");
        assert_eq!(q(&|q| q.untagged = true), "Bolt,Opal,Zap", "Zap: a purchase whose only tag is a lot's is untagged");
        assert_eq!(q(&|q| q.has_note = true), "Zap", "a blank note isn't a note");
        assert_eq!(q(&|q| q.price_min = Some(1.0)), "Bolt,Opal");
        assert_eq!(q(&|q| q.price_max = Some(1.0)), "Bolt,Zap", "unpriced Shock matches no bound");
        assert_eq!(
            q(&|q| {
                q.price_min = Some(0.5);
                q.price_max = Some(0.5)
            }),
            "Zap"
        );
        assert_eq!(q(&|q| q.acquired_from = "2020-01-01".into()), "Bolt,Zap");
        assert_eq!(q(&|q| q.acquired_to = "2020-05-01".into()), "Bolt,Opal", "the bounds are inclusive; undated never matches");
        assert_eq!(
            q(&|q| {
                q.acquired_from = "2019-06-01".into();
                q.acquired_to = "2020-12-31".into()
            }),
            "Bolt"
        );
        assert_eq!(q(&|q| q.min_copies = Some(3)), "Bolt,Shock,Zap");
        assert_eq!(q(&|q| q.no_price_paid = true), "Bolt,Shock,Zap", "any copy without a price paid");
        assert_eq!(q(&|q| q.unpriced = true), "Shock,Zap", "Zap's foil has no price");
    }

    #[test]
    fn tag_filters() {
        // Bolt: one purchase "elves deck", another "Binder A". Shock: "binder a" and an
        // untagged purchase. Opal: untagged. Zap: "for sale".
        let mut bolt = card("Bolt", "M11", "149", "common", "nonfoil");
        tag(&mut bolt, &["elves deck"]);
        bolt.add_purchase("foil", Condition::NearMint, Purchase { quantity: 1, tags: vec!["Binder A".into()], ..Default::default() });
        let mut shock = card("Shock", "M19", "156", "common", "nonfoil");
        shock.add_purchase("foil", Condition::NearMint, Purchase { quantity: 1, tags: vec!["binder a".into()], ..Default::default() });
        let opal = card("Opal", "USG", "10", "rare", "nonfoil");
        let mut zap = card("Zap", "M19", "10", "common", "nonfoil");
        tag(&mut zap, &["for sale"]);
        let cards = [bolt, shock, opal, zap];
        let q = |f: &dyn Fn(&mut Query)| {
            let mut q = Query::default();
            f(&mut q);
            let m = q.matcher();
            names(&cards.iter().filter(|c| m(c)).cloned().collect::<Vec<_>>()).join(",")
        };
        let v = |t: &[&str]| t.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(q(&|q| q.tags = v(&["BINDER A"])), "Bolt,Shock", "ignoring case");
        assert_eq!(q(&|q| q.tags = v(&["elves deck", "binder a"])), "Bolt", "all, across purchases");
        assert_eq!(
            q(&|q| {
                q.tags = v(&["elves deck", "for sale"]);
                q.tags_any = true
            }),
            "Bolt,Zap"
        );
        assert_eq!(q(&|q| q.tags_not = v(&["binder a"])), "Opal,Zap");
        assert_eq!(
            q(&|q| {
                q.tags = v(&["binder a"]);
                q.tags_not = v(&["elves deck"])
            }),
            "Shock"
        );
        assert_eq!(q(&|q| q.untagged = true), "Shock,Opal", "a purchase with no tags");
        assert_eq!(q(&|q| q.tags = v(&["nobody"])), "");
        assert_eq!(q(&|q| q.text = "sale".into()), "Zap", "search matches tags");
    }
}
