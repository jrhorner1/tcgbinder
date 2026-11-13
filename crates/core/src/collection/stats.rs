use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use super::{Card, SetInfo};
use crate::dates;

/// A summary of the owned cards of one set.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SetStats {
    pub code: String,
    pub name: String,
    /// `YYYY-MM-DD`.
    pub released: String,
    pub unique_cards: usize,
    pub cards: i64,
    /// Distinct collector numbers owned, in any finish or condition.
    pub collected: usize,
    /// Cards in the set per the catalog; 0 if unknown.
    pub set_size: usize,
    pub price_paid: f64,
    pub market_value: f64,
}

impl SetStats {
    /// The owned fraction of the set (0..1), or `None` when the set size is unknown.
    pub fn completion(&self) -> Option<f64> {
        (self.set_size > 0).then(|| (self.collected as f64 / self.set_size as f64).min(1.0))
    }
}

/// A card with its value: one copy (its most valuable owned finish, or the average)
/// and every copy.
#[derive(Clone, Debug, PartialEq)]
pub struct CardValue {
    pub card: Card,
    pub each: f64,
    pub total: f64,
}

/// One value on a time series.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimePoint {
    pub date: NaiveDate,
    pub value: f64,
}

/// Everything the Home dashboard shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stats {
    pub cards: i64,
    pub unique_cards: usize,
    pub sets: usize,
    /// The total price paid, for copies with a known price.
    pub price_paid: f64,
    /// Copies with no price paid recorded.
    pub price_paid_unknown: i64,
    /// The total current market value.
    pub market_value: f64,
    /// Copies with no market price.
    pub unpriced: i64,
    /// The oldest price refresh date across cards.
    pub prices_as_of: String,
    /// Newest set first.
    pub by_set: Vec<SetStats>,
    pub top_cards: Vec<CardValue>,
    /// The cumulative price paid by acquisition date, through today.
    pub price_paid_history: Vec<TimePoint>,
    /// The value of the cards owned on each date, at today's market prices (by
    /// acquisition date, through today). It shows how the collection grew, not how
    /// prices moved.
    pub value_history: Vec<TimePoint>,
}

/// How many of the most valuable cards [`Stats`] lists.
const TOP_CARDS: usize = 10;

/// Inserts `v` into `top` (most valuable first), keeping at most `n`, so finding the
/// top cards doesn't copy and sort every card. Equal totals keep their order.
fn keep_top(top: &mut Vec<CardValue>, v: CardValue, n: usize) {
    let i = top.partition_point(|t| t.total >= v.total);
    if i < n {
        top.insert(i, v);
        top.truncate(n);
    }
}

/// Aggregates the collection. `sets` may be empty when set sizes aren't loaded.
pub fn compute_stats(cards: &[Card], sets: &HashMap<String, SetInfo>) -> Stats {
    compute_stats_at(cards, sets, dates::today())
}

/// [`compute_stats`] with the history series ending on `today`.
pub fn compute_stats_at(cards: &[Card], sets: &HashMap<String, SetInfo>, today: NaiveDate) -> Stats {
    let mut st = Stats::default();
    let mut by_set: HashMap<String, SetStats> = HashMap::new();
    let mut numbers: HashMap<String, HashSet<&str>> = HashMap::new();
    let (mut paid_by_date, mut value_by_date) = (BTreeMap::new(), BTreeMap::new());
    // Cards with no acquired date count from the start.
    let (mut paid_undated, mut value_undated) = (0.0, 0.0);

    for c in cards {
        let qty = c.copy_count();
        st.cards += qty;
        st.unique_cards += 1;

        let code = c.set.to_uppercase();
        let info = sets.get(&code);
        let s = by_set.entry(code.clone()).or_insert_with(|| {
            let mut s = SetStats { code: code.clone(), name: c.set_name.clone(), released: c.released_at.clone(), ..Default::default() };
            if let Some(info) = info {
                s.set_size = info.card_count;
                s.released = info.released_at.clone();
                if !info.name.is_empty() {
                    s.name = info.name.clone();
                }
            }
            s
        });
        s.unique_cards += 1;
        s.cards += qty;
        numbers.entry(code).or_default().insert(&c.collector_number);
        if info.is_none() && !c.released_at.is_empty() && (s.released.is_empty() || c.released_at < s.released) {
            s.released = c.released_at.clone(); // no set metadata: use the earliest card release
        }

        let mut card_value = 0.0;
        let mut priced = false;
        for (finish, _, p) in c.copies.each() {
            // Each finish and condition, with its own date.
            let date = dates::parse(&p.acquired_date);
            if p.price_paid > 0.0 {
                let paid = p.price_paid * p.quantity as f64;
                st.price_paid += paid;
                s.price_paid += paid;
                match date {
                    Some(d) => *paid_by_date.entry(d).or_insert(0.0) += paid,
                    None => paid_undated += paid,
                }
            } else {
                st.price_paid_unknown += p.quantity;
            }
            match c.market_price(finish) {
                Some(price) => {
                    let total = price * p.quantity as f64;
                    st.market_value += total;
                    s.market_value += total;
                    card_value += total;
                    priced = true;
                    match date {
                        Some(d) => *value_by_date.entry(d).or_insert(0.0) += total,
                        None => value_undated += total,
                    }
                }
                None => st.unpriced += p.quantity,
            }
        }
        if priced {
            let each = if qty > 0 { card_value / qty as f64 } else { 0.0 };
            keep_top(&mut st.top_cards, CardValue { card: c.clone(), each, total: card_value }, TOP_CARDS);
        }
        if !c.prices_updated.is_empty() && (st.prices_as_of.is_empty() || c.prices_updated < st.prices_as_of) {
            st.prices_as_of = c.prices_updated.clone();
        }
    }

    st.by_set = by_set
        .into_values()
        .map(|mut s| {
            s.collected = numbers.get(&s.code).map_or(0, HashSet::len);
            s
        })
        .collect();
    st.sets = st.by_set.len();
    // Newest set first.
    st.by_set.sort_by(|a, b| b.released.cmp(&a.released).then_with(|| a.code.cmp(&b.code)));
    st.price_paid_history = cumulative(&paid_by_date, paid_undated, today);
    st.value_history = cumulative(&value_by_date, value_undated, today);
    st
}

/// Turns per-date amounts into a running total by date. Undated amounts count from the
/// first point, and the series is extended to `today` so it ends at the current total.
fn cumulative(by_date: &BTreeMap<NaiveDate, f64>, undated: f64, today: NaiveDate) -> Vec<TimePoint> {
    if by_date.is_empty() && undated == 0.0 {
        return Vec::new();
    }
    let mut running = undated;
    let mut out: Vec<TimePoint> = by_date
        .iter()
        .map(|(d, v)| {
            running += v;
            TimePoint { date: *d, value: running }
        })
        .collect();
    if out.last().is_none_or(|p| p.date < today) {
        out.push(TimePoint { date: today, value: running });
    }
    out
}

/// The collection's totals on one day, for the value-over-time chart.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ValueSnapshot {
    /// `YYYY-MM-DD`.
    pub date: String,
    pub market_value: f64,
    pub price_paid: f64,
    pub cards: i64,
}

/// Replaces the snapshot for `snap.date` or adds it, keeping date order.
pub(crate) fn upsert_snapshot(history: &mut Vec<ValueSnapshot>, snap: ValueSnapshot) {
    match history.iter_mut().find(|h| h.date == snap.date) {
        Some(h) => *h = snap,
        None => {
            history.push(snap);
            history.sort_by(|a, b| a.date.cmp(&b.date));
        }
    }
}

/// Rounds dollars to whole cents.
pub fn round_cents(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::{Condition, Purchase};

    fn day(s: &str) -> NaiveDate {
        dates::parse(s).unwrap()
    }

    fn card(id: &str, set: &str, number: &str, buys: &[(i64, f64, &str)], market: Option<f64>) -> Card {
        let mut c = Card {
            id: id.into(),
            name: id.into(),
            set: set.into(),
            set_name: format!("{set} Set"),
            collector_number: number.into(),
            released_at: "2020-01-01".into(),
            prices_updated: "2024-05-01".into(),
            ..Card::default()
        };
        for (q, paid, date) in buys {
            c.add_purchase("nonfoil", Condition::NearMint, Purchase { quantity: *q, price_paid: *paid, acquired_date: date.to_string() });
        }
        if let Some(m) = market {
            c.market.insert("nonfoil".into(), m);
        }
        c
    }

    #[test]
    fn totals_sets_and_history() {
        let cards = vec![
            card("a", "m19", "1", &[(2, 1.0, "2024-01-10"), (1, 0.0, "")], Some(3.0)),
            card("b", "M19", "2", &[(1, 5.0, "2024-02-01")], None),
            card("c", "XLN", "1", &[(4, 0.5, "bad date")], Some(0.25)),
        ];
        let sets: HashMap<String, SetInfo> = [(
            "M19".to_string(),
            SetInfo { code: "m19".into(), name: "Core Set 2019".into(), card_count: 280, released_at: "2018-07-13".into() },
        )]
        .into();
        let st = compute_stats_at(&cards, &sets, day("2024-03-01"));
        assert_eq!((st.cards, st.unique_cards, st.sets), (8, 3, 2));
        assert_eq!((st.price_paid, st.price_paid_unknown), (9.0, 1));
        assert_eq!((st.market_value, st.unpriced), (10.0, 1));
        assert_eq!(st.prices_as_of, "2024-05-01");

        let m19 = st.by_set.iter().find(|s| s.code == "M19").unwrap();
        assert_eq!((m19.name.as_str(), m19.collected, m19.set_size, m19.cards), ("Core Set 2019", 2, 280, 4));
        assert_eq!(m19.completion(), Some(2.0 / 280.0));
        assert_eq!(st.by_set[0].code, "XLN"); // released 2020, after M19's 2018
        assert_eq!(st.by_set[0].completion(), None);

        assert_eq!(st.top_cards.iter().map(|t| t.card.id.as_str()).collect::<Vec<_>>(), ["a", "c"]);
        assert_eq!(st.top_cards[0].each, 3.0);

        let paid: Vec<(String, f64)> = st.price_paid_history.iter().map(|p| (dates::format(p.date), p.value)).collect();
        assert_eq!(paid, [("2024-01-10".into(), 4.0), ("2024-02-01".into(), 9.0), ("2024-03-01".into(), 9.0)]);
        let value: Vec<f64> = st.value_history.iter().map(|p| p.value).collect();
        assert_eq!(value, [10.0, 10.0]); // undated copies (c, and one of a) count from the start
    }

    #[test]
    fn top_cards_capped() {
        let cards: Vec<Card> =
            (0..15).map(|i| card(&format!("c{i:02}"), "S", &i.to_string(), &[(1, 0.0, "")], Some(f64::from(i)))).collect();
        let st = compute_stats_at(&cards, &HashMap::new(), day("2024-01-01"));
        assert_eq!(st.top_cards.len(), TOP_CARDS);
        assert_eq!(st.top_cards[0].card.id, "c14");
        assert_eq!(st.top_cards[9].card.id, "c05");
    }

    #[test]
    fn snapshots_upsert() {
        let mut h = Vec::new();
        upsert_snapshot(&mut h, ValueSnapshot { date: "2024-01-02".into(), cards: 1, ..Default::default() });
        upsert_snapshot(&mut h, ValueSnapshot { date: "2024-01-01".into(), cards: 2, ..Default::default() });
        upsert_snapshot(&mut h, ValueSnapshot { date: "2024-01-02".into(), cards: 3, ..Default::default() });
        assert_eq!(h.iter().map(|s| s.cards).collect::<Vec<_>>(), [2, 3]);
        assert_eq!(round_cents(2.346), 2.35);
    }
}
