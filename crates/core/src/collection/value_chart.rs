//! The Value Over Time chart on Home: the collection's market value from recorded
//! prices, the running price paid, and an estimate at today's prices before recorded
//! prices begin, over a chosen range.

use chrono::{Months, NaiveDate};

use super::{Card, PriceHistory, Stats, TimePoint};

/// The Value Over Time range shown first (see [`crate::settings::choices::CHART_RANGES`]), matching the 90 days of history MTGJSON provides.
pub const DEFAULT_CHART_RANGE: &str = "90D";

/// The first day shown for a range; `None` means everything.
pub fn range_start(range: &str, today: NaiveDate) -> Option<NaiveDate> {
    match range {
        "30D" => Some(today - chrono::Duration::days(30)),
        "90D" => Some(today - chrono::Duration::days(90)),
        "1Y" => today.checked_sub_months(Months::new(12)),
        _ => None,
    }
}

/// The data behind the Value Over Time chart.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ValueChart {
    /// `None` for "All".
    pub from: Option<NaiveDate>,
    pub to: NaiveDate,
    /// The first day with recorded prices.
    pub history_start: Option<NaiveDate>,
    /// The real daily market value.
    pub market_value: Vec<TimePoint>,
    /// The running price paid (a step line).
    pub price_paid: Vec<TimePoint>,
    /// Values cards at today's prices before recorded prices begin (a step line); empty
    /// when the real line covers the whole range.
    pub estimate: Vec<TimePoint>,
}

/// Computes the chart for `cards` over the named range.
pub fn build_value_chart(cards: &[Card], st: &Stats, prices: &PriceHistory, range: &str, today: NaiveDate) -> ValueChart {
    let from = range_start(range, today);
    let history_start = prices.start();
    let mut vc = ValueChart { from, to: today, history_start, ..Default::default() };
    if let Some(market_from) = from.or(history_start) {
        vc.market_value = prices.market_series(cards, market_from, today);
    }
    vc.price_paid = clip_step(&st.price_paid_history, from, today);
    let before_history = match (history_start, from) {
        (Some(h), Some(f)) => f < h,
        _ => true,
    };
    if before_history {
        let est = clip_step(&st.value_history, from, today);
        vc.estimate = match history_start {
            Some(h) => end_at(&est, h), // real prices take over from here
            None => est,
        };
    }
    vc
}

/// Limits a step series to dates on or after `from`, starting with the value in effect
/// on that day so the line doesn't begin at zero mid-range.
pub fn clip_step(points: &[TimePoint], from: Option<NaiveDate>, today: NaiveDate) -> Vec<TimePoint> {
    let Some(from) = from else { return points.to_vec() };
    let mut out = Vec::new();
    for (i, p) in points.iter().enumerate() {
        if p.date > from {
            if out.is_empty() && i > 0 {
                out.push(TimePoint { date: from, value: points[i - 1].value });
            }
            out.push(*p);
        } else if p.date == from {
            out.push(*p);
        }
    }
    if let (true, Some(last)) = (out.is_empty(), points.last()) {
        // Everything happened before the range.
        let end = if last.date > from { last.date } else { today };
        out = vec![TimePoint { date: from, value: last.value }, TimePoint { date: end, value: last.value }];
    }
    out
}

/// Keeps a step series' points before `until` and ends it with the value in effect on
/// that day.
pub fn end_at(points: &[TimePoint], until: NaiveDate) -> Vec<TimePoint> {
    let mut out: Vec<TimePoint> = points.iter().take_while(|p| p.date < until).copied().collect();
    if let Some(last) = out.last().copied() {
        out.push(TimePoint { date: until, value: last.value });
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::collection::{Condition, Purchase, compute_stats_at};
    use crate::dates;

    fn day(s: &str) -> NaiveDate {
        dates::parse(s).unwrap()
    }

    fn pts(v: &[TimePoint]) -> Vec<(String, f64)> {
        v.iter().map(|p| (dates::format(p.date), p.value)).collect()
    }

    fn tp(d: &str, v: f64) -> TimePoint {
        TimePoint { date: day(d), value: v }
    }

    #[test]
    fn ranges() {
        let t = day("2024-03-31");
        assert_eq!(range_start("30D", t), Some(day("2024-03-01")));
        assert_eq!(range_start("1Y", t), Some(day("2023-03-31")));
        assert_eq!(range_start("All", t), None);
    }

    #[test]
    fn clip_and_end() {
        let s = [tp("2024-01-01", 1.0), tp("2024-02-01", 2.0), tp("2024-03-01", 3.0)];
        let today = day("2024-04-01");
        assert_eq!(
            pts(&clip_step(&s, Some(day("2024-01-15")), today)),
            [("2024-01-15".into(), 1.0), ("2024-02-01".into(), 2.0), ("2024-03-01".into(), 3.0)]
        );
        assert_eq!(pts(&clip_step(&s, Some(day("2024-03-15")), today)), [("2024-03-15".into(), 3.0), ("2024-04-01".into(), 3.0)]);
        assert_eq!(clip_step(&s, None, today).len(), 3);
        assert_eq!(
            pts(&end_at(&s, day("2024-02-15"))),
            [("2024-01-01".into(), 1.0), ("2024-02-01".into(), 2.0), ("2024-02-15".into(), 2.0)]
        );
        assert!(end_at(&s, day("2023-01-01")).is_empty());
    }

    #[test]
    fn chart_estimates_before_history() {
        let mut c = Card { id: "a".into(), ..Card::default() };
        c.add_purchase(
            "nonfoil",
            Condition::NearMint,
            Purchase { quantity: 1, price_paid: 2.0, acquired_date: "2024-01-01".into(), ..Default::default() },
        );
        c.market.insert("nonfoil".into(), 5.0);
        let cards = vec![c];
        let today = day("2024-03-01");
        let prices = PriceHistory::in_memory();
        prices.set("a", "nonfoil", "2024-02-20", 4.0);
        let st = compute_stats_at(&cards, &HashMap::new(), true, today);
        let vc = build_value_chart(&cards, &st, &prices, "30D", today);
        assert_eq!(vc.history_start, Some(day("2024-02-20")));
        assert_eq!(pts(&vc.market_value), [("2024-02-20".into(), 4.0), ("2024-03-01".into(), 4.0)]);
        assert_eq!(pts(&vc.estimate), [("2024-01-31".into(), 5.0), ("2024-02-20".into(), 5.0)]);
        assert_eq!(pts(&vc.price_paid), [("2024-01-31".into(), 2.0), ("2024-03-01".into(), 2.0)]);

        let all = build_value_chart(&cards, &st, &PriceHistory::in_memory(), "All", today);
        assert!(all.market_value.is_empty());
        assert_eq!(all.estimate.len(), 2);
    }
}
