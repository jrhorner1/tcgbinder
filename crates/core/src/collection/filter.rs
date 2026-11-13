use std::cmp::Ordering;
use std::collections::BTreeMap;

use super::Card;

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
}

impl Query<'_> {
    /// Tells whether a card matches the query.
    pub fn matcher(&self) -> impl Fn(&Card) -> bool + '_ {
        let terms: Vec<String> = self.text.to_lowercase().split_whitespace().map(str::to_string).collect();
        move |c| {
            (is_any(&self.set) || c.set.eq_ignore_ascii_case(&self.set))
                && (is_any(&self.rarity) || c.rarity.eq_ignore_ascii_case(&self.rarity))
                && (is_any(&self.finish) || c.has_finish(&self.finish))
                && self.matches.as_ref().is_none_or(|m| m(c))
                // Each term in the name, set code, set name or number.
                && terms.iter().all(|t| [&c.name, &c.set, &c.set_name, &c.collector_number].iter().any(|field| contains_fold(field, t)))
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

/// The cards in one of the Card orders (the Card order setting): "newest" or "oldest" by
/// set release, then collector number; "value" for the most valuable first (every copy);
/// anything else by name.
pub fn sort_cards(mut cards: Vec<Card>, card_order: &str) -> Vec<Card> {
    let by_set = |a: &Card, b: &Card, newest_first: bool| {
        let released = a.released_at.cmp(&b.released_at);
        (if newest_first { released.reverse() } else { released })
            .then_with(|| compare_fold(&a.set, &b.set))
            .then_with(|| compare_collector_number(&a.collector_number, &b.collector_number))
            .then_with(|| default_order(a, b))
    };
    cards.sort_by(|a, b| match card_order {
        "newest" => by_set(a, b, true),
        "oldest" => by_set(a, b, false),
        "value" => b.market_value().0.total_cmp(&a.market_value().0).then_with(|| default_order(a, b)),
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
            names(&sort_cards(matched, "name")).join(",")
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
        assert_eq!(names(&sort_cards(cards.clone(), "newest")), ["Shock", "Bolt"]);
        assert_eq!(names(&sort_cards(cards.clone(), "oldest")), ["Bolt", "Shock"]);
        assert_eq!(names(&sort_cards(cards.clone(), "value")), ["Bolt", "Shock"]);
        assert_eq!(names(&sort_cards(cards, "name")), ["Bolt", "Shock"]);
    }
}
