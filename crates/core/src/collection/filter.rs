use std::cmp::Ordering;
use std::collections::BTreeMap;

use super::Card;

/// A column the collection list can be ordered by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortField {
    #[default]
    Name,
    Set,
    Number,
    Rarity,
    Quantity,
    Price,
    Acquired,
    Released,
}

/// The "no filter" choice in filter menus.
pub const FILTER_ANY: &str = "Any";

/// Whether a filter value means "no filter".
pub fn is_any(s: &str) -> bool {
    s.is_empty() || s == FILTER_ANY
}

/// An extra, game-specific match on a card.
pub type CardMatch<'a> = Box<dyn Fn(&Card) -> bool + 'a>;

/// How to narrow and order the collection view. The default matches everything,
/// sorted by name.
#[derive(Default)]
pub struct Query<'a> {
    /// Whitespace-separated terms; every term must match the name, set code, set name
    /// or collector number.
    pub text: String,
    /// Set code (case-insensitive); "" or [`FILTER_ANY`] for all.
    pub set: String,
    pub rarity: String,
    /// The finish (case-insensitive); "" or [`FILTER_ANY`] for all.
    pub finish: String,
    /// An extra, game-specific condition (e.g. Magic's color filter).
    pub matches: Option<CardMatch<'a>>,
    /// Ranks rarities for [`SortField::Rarity`], lowest first; others sort after them.
    pub rarity_order: Vec<String>,
    pub sort_by: SortField,
    pub descending: bool,
}

/// The cards that match `q`, ordered per `q`.
pub fn apply_query(cards: &[Card], q: &Query) -> Vec<Card> {
    let terms: Vec<String> = q.text.to_lowercase().split_whitespace().map(str::to_string).collect();
    let mut matched: Vec<&Card> = cards.iter().filter(|c| matches_query(c, q, &terms)).collect();
    matched.sort_by(|a, b| {
        let cmp = compare_by(a, b, q.sort_by, &q.rarity_order).then_with(|| default_order(a, b));
        if q.descending { cmp.reverse() } else { cmp }
    });
    matched.into_iter().cloned().collect()
}

fn matches_query(c: &Card, q: &Query, terms: &[String]) -> bool {
    if !is_any(&q.set) && !c.set.eq_ignore_ascii_case(&q.set) {
        return false;
    }
    if !is_any(&q.rarity) && !c.rarity.eq_ignore_ascii_case(&q.rarity) {
        return false;
    }
    if !is_any(&q.finish) && !c.has_finish(&q.finish) {
        return false;
    }
    if let Some(m) = &q.matches
        && !m(c)
    {
        return false;
    }
    // Each term in the name, set code, set name or number.
    terms.iter().all(|t| [&c.name, &c.set, &c.set_name, &c.collector_number].iter().any(|field| contains_fold(field, t)))
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

fn compare_by(a: &Card, b: &Card, field: SortField, rarity_order: &[String]) -> Ordering {
    match field {
        SortField::Set => compare_fold(&a.set_name, &b.set_name),
        SortField::Number => compare_collector_number(&a.collector_number, &b.collector_number),
        SortField::Rarity => rank(rarity_order, &a.rarity).cmp(&rank(rarity_order, &b.rarity)),
        SortField::Quantity => a.copy_count().cmp(&b.copy_count()),
        SortField::Price => a.price_paid().0.total_cmp(&b.price_paid().0),
        SortField::Acquired => a.earliest_acquired().cmp(b.earliest_acquired()),
        SortField::Released => a.released_at.cmp(&b.released_at),
        SortField::Name => compare_fold(&a.name, &b.name),
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

/// `v`'s position in `order` (case-insensitive), or `order.len()` if absent.
fn rank(order: &[String], v: &str) -> usize {
    order.iter().position(|o| o.eq_ignore_ascii_case(v)).unwrap_or(order.len())
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

/// The card orders for the collection list (the Card order setting).
pub mod order {
    /// By name.
    pub const NAME: &str = "name";
    /// By set, newest release first, then collector number.
    pub const NEWEST: &str = "newest";
    /// By set, oldest release first, then collector number.
    pub const OLDEST: &str = "oldest";
    /// Most valuable first (every copy), then by name.
    pub const VALUE: &str = "value";
}

/// The cards in one of the Card orders.
pub fn sort_cards(mut cards: Vec<Card>, card_order: &str) -> Vec<Card> {
    let by_set = |a: &Card, b: &Card, newest_first: bool| {
        let released = a.released_at.cmp(&b.released_at);
        (if newest_first { released.reverse() } else { released })
            .then_with(|| compare_fold(&a.set, &b.set))
            .then_with(|| compare_collector_number(&a.collector_number, &b.collector_number))
            .then_with(|| default_order(a, b))
    };
    cards.sort_by(|a, b| match card_order {
        order::NEWEST => by_set(a, b, true),
        order::OLDEST => by_set(a, b, false),
        order::VALUE => b.market_value().0.total_cmp(&a.market_value().0).then_with(|| default_order(a, b)),
        _ => default_order(a, b),
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

    fn names(cards: &[Card]) -> Vec<&str> {
        cards.iter().map(|c| c.name.as_str()).collect()
    }

    #[test]
    fn filters_and_terms() {
        let cards = vec![
            card("Lightning Bolt", "M11", "149", "common", "nonfoil"),
            card("Shock", "M19", "156", "common", "foil"),
            card("Scapeshift", "M19", "201", "rare", "nonfoil"),
            card("Éclair", "ABC", "1", "rare", "nonfoil"),
        ];
        let q = |f: &dyn Fn(&mut Query)| {
            let mut q = Query::default();
            f(&mut q);
            names(&apply_query(&cards, &q)).join(",")
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
        assert_eq!(
            q(&|q| {
                q.sort_by = SortField::Number;
                q.descending = true;
            }),
            "Scapeshift,Shock,Lightning Bolt,Éclair"
        );
        assert_eq!(
            q(&|q| {
                q.sort_by = SortField::Rarity;
                q.rarity_order = vec!["common".into(), "rare".into()];
            }),
            "Lightning Bolt,Shock,Scapeshift,Éclair"
        );
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
        assert_eq!(names(&sort_cards(cards.clone(), order::NEWEST)), ["Shock", "Bolt"]);
        assert_eq!(names(&sort_cards(cards.clone(), order::OLDEST)), ["Bolt", "Shock"]);
        assert_eq!(names(&sort_cards(cards.clone(), order::VALUE)), ["Bolt", "Shock"]);
        assert_eq!(names(&sort_cards(cards, order::NAME)), ["Bolt", "Shock"]);
    }
}
