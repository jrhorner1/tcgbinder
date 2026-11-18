//! The Statistics page's figures: the collection broken down by rarity, finish,
//! condition, language, release year and the game's own groupings, by value band,
//! and by month acquired. One pass over the cards for each.

use std::collections::BTreeMap;

use serde::Serialize;

use super::{Card, Condition, round_cents};

/// A way to group cards (a game adds its own, e.g. Magic's colors). A card counts
/// under every label it has.
pub struct Breakdown {
    pub name: &'static str,
    /// The collection list's filter parameter for a row (its key as the value), if any.
    pub param: Option<&'static str>,
    /// The rows' order; rows not in it follow, most cards first. Empty: most cards first.
    pub order: &'static [&'static str],
    pub labels: fn(&Card) -> Vec<String>,
}

/// One row of a breakdown: `key` (as stored, e.g. "rare"), copies (`cards`), printings
/// (`unique`), market value and price paid.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct BreakdownRow {
    pub key: String,
    pub cards: i64,
    pub unique: usize,
    pub value: f64,
    pub paid: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct BreakdownTable {
    /// A stable ID for the UI ("rarity", "finish", …, or the game breakdown's name).
    pub id: String,
    pub name: String,
    pub param: Option<String>,
    pub rows: Vec<BreakdownRow>,
}

/// Cards whose one-copy price falls in a band.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct ValueBand {
    pub label: String,
    /// Bounds on one copy's price, for the collection list's filter: `min` ≤ price < `max`.
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub cards: i64,
    pub unique: usize,
    pub value: f64,
    /// Share of the collection's market value, 0 to 1.
    pub share: f64,
}

/// Copies acquired and money spent in a month (`YYYY-MM`).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct MonthRow {
    pub month: String,
    pub cards: i64,
    pub paid: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Statistics {
    pub breakdowns: Vec<BreakdownTable>,
    pub bands: Vec<ValueBand>,
    pub by_month: Vec<MonthRow>,
    /// Copies with no acquired date, left out of `by_month`.
    pub undated: i64,
}

/// Adds up rows by key.
#[derive(Default)]
struct Tally(BTreeMap<String, (i64, std::collections::HashSet<String>, f64, f64)>);

impl Tally {
    fn add(&mut self, key: &str, card_id: &str, copies: i64, value: f64, paid: f64) {
        let e = self.0.entry(key.to_string()).or_default();
        e.0 += copies;
        e.1.insert(card_id.to_string());
        e.2 += value;
        e.3 += paid;
    }

    /// The rows in `order`, then the rest by most cards.
    fn rows(self, order: &[&str]) -> Vec<BreakdownRow> {
        let mut rows: Vec<BreakdownRow> = self
            .0
            .into_iter()
            .map(|(key, (cards, ids, value, paid))| BreakdownRow {
                key,
                cards,
                unique: ids.len(),
                value: round_cents(value),
                paid: round_cents(paid),
            })
            .collect();
        let rank = |k: &str| order.iter().position(|o| o.eq_ignore_ascii_case(k)).unwrap_or(order.len());
        rows.sort_by(|a, b| rank(&a.key).cmp(&rank(&b.key)).then_with(|| b.cards.cmp(&a.cards)).then_with(|| a.key.cmp(&b.key)));
        rows
    }
}

/// The bands of one copy's price.
const BANDS: &[(&str, f64, Option<f64>)] = &[
    ("Under $1", 0.0, Some(1.0)),
    ("$1 to $5", 1.0, Some(5.0)),
    ("$5 to $20", 5.0, Some(20.0)),
    ("$20 to $100", 20.0, Some(100.0)),
    ("$100 and up", 100.0, None),
];

/// The collection's statistics. `extra` are the game's own breakdowns; `rarities` and
/// `finishes` are the game's orders for those rows.
pub fn statistics(cards: &[Card], extra: &[Breakdown], rarities: &[&str], finishes: &[&str]) -> Statistics {
    let (mut rarity, mut finish, mut condition, mut language, mut year) =
        (Tally::default(), Tally::default(), Tally::default(), Tally::default(), Tally::default());
    let mut game: Vec<Tally> = extra.iter().map(|_| Tally::default()).collect();
    let mut bands: Vec<ValueBand> = BANDS
        .iter()
        .map(|(label, min, max)| ValueBand {
            label: label.to_string(),
            min: (*min > 0.0).then_some(*min),
            max: *max,
            ..ValueBand::default()
        })
        .collect();
    let mut unpriced = ValueBand { label: "No price".into(), ..ValueBand::default() };
    let mut months: BTreeMap<String, MonthRow> = BTreeMap::new();
    let mut undated = 0;
    let mut total_value = 0.0;

    for c in cards {
        let copies = c.copy_count();
        if copies <= 0 {
            continue;
        }
        let (value, _) = c.market_value();
        let (paid, _) = c.price_paid();
        total_value += value;
        rarity.add(&c.rarity.to_lowercase(), &c.id, copies, value, paid);
        language.add(if c.language.is_empty() { "en" } else { &c.language }, &c.id, copies, value, paid);
        if c.released_at.len() >= 4 {
            year.add(&c.released_at[..4], &c.id, copies, value, paid);
        }
        for (t, b) in game.iter_mut().zip(extra) {
            for label in (b.labels)(c) {
                t.add(&label, &c.id, copies, value, paid);
            }
        }
        // Per purchase: finish, condition, month.
        for (f, cond, p) in c.copies.each() {
            let v = c.market.get(f).copied().unwrap_or(0.0) * p.quantity as f64;
            let pd = p.price_paid * p.quantity as f64;
            finish.add(f, &c.id, p.quantity, v, pd);
            condition.add(cond.code(), &c.id, p.quantity, v, pd);
            match p.acquired_date.get(..7) {
                Some(m) if p.acquired_date.len() == 10 => {
                    let row = months.entry(m.to_string()).or_insert_with(|| MonthRow { month: m.to_string(), ..MonthRow::default() });
                    row.cards += p.quantity;
                    row.paid += pd;
                }
                _ => undated += p.quantity,
            }
        }
        let band = match c.best_price() {
            None => &mut unpriced,
            Some(price) => {
                let i = BANDS.iter().rposition(|(_, min, _)| price >= *min).unwrap_or(0);
                &mut bands[i]
            }
        };
        band.cards += copies;
        band.unique += 1;
        band.value += value;
    }

    if unpriced.cards > 0 {
        bands.push(unpriced);
    }
    for b in &mut bands {
        b.share = if total_value > 0.0 { (b.value / total_value * 1000.0).round() / 1000.0 } else { 0.0 };
        b.value = round_cents(b.value);
    }
    let conditions: Vec<&str> = Condition::ALL.iter().map(|c| c.code()).collect();
    let table = |id: &str, name: &str, param: Option<&str>, rows: Vec<BreakdownRow>| BreakdownTable {
        id: id.into(),
        name: name.into(),
        param: param.map(str::to_string),
        rows,
    };
    let mut years = year.rows(&[]);
    years.sort_by(|a, b| b.key.cmp(&a.key));
    let mut breakdowns = vec![
        table("rarity", "Rarity", Some("rarity"), rarity.rows(rarities)),
        table("finish", "Finish", Some("finish"), finish.rows(finishes)),
        table("condition", "Condition", Some("condition"), condition.rows(&conditions)),
        table("language", "Language", Some("language"), language.rows(&[])),
        table("year", "Release year", None, years),
    ];
    for (t, b) in game.into_iter().zip(extra) {
        breakdowns.push(table(b.name, b.name, b.param, t.rows(b.order)));
    }
    let by_month = months.into_values().map(|mut m| {
        m.paid = round_cents(m.paid);
        m
    });
    Statistics { breakdowns, bands, by_month: by_month.collect(), undated }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::Purchase;

    fn card(id: &str, rarity: &str, released: &str, prices: &[(&str, f64)]) -> Card {
        Card {
            id: id.into(),
            name: id.into(),
            rarity: rarity.into(),
            released_at: released.into(),
            language: "en".into(),
            market: prices.iter().map(|(f, p)| (f.to_string(), *p)).collect(),
            ..Card::default()
        }
    }

    fn buy(c: &mut Card, finish: &str, cond: Condition, q: i64, paid: f64, date: &str) {
        c.add_purchase(finish, cond, Purchase { quantity: q, price_paid: paid, acquired_date: date.into(), ..Default::default() });
    }

    fn row<'a>(s: &'a Statistics, table: &str, key: &str) -> &'a BreakdownRow {
        s.breakdowns.iter().find(|t| t.id == table).unwrap().rows.iter().find(|r| r.key == key).unwrap_or_else(|| panic!("{table} {key}"))
    }

    #[test]
    fn breakdowns_bands_and_months() {
        // Bolt: 3 non-foil NM at $0.50 (paid 0.25, 2024-03) and 1 foil LP at $4 (paid 2, undated).
        let mut bolt = card("bolt", "common", "2010-07-16", &[("nonfoil", 0.5), ("foil", 4.0)]);
        buy(&mut bolt, "nonfoil", Condition::NearMint, 3, 0.25, "2024-03-10");
        buy(&mut bolt, "foil", Condition::LightPlay, 1, 2.0, "");
        // Ring: 1 non-foil NM at $150 (paid 100, 2024-03), Japanese.
        let mut ring = card("ring", "mythic", "2023-06-23", &[("nonfoil", 150.0)]);
        ring.language = "ja".into();
        buy(&mut ring, "nonfoil", Condition::NearMint, 1, 100.0, "2024-03-20");
        // Token: 2 copies, no price, 2024-01.
        let mut token = card("token", "special", "2023-06-23", &[]);
        buy(&mut token, "nonfoil", Condition::NearMint, 2, 0.0, "2024-01-05");

        let colors = Breakdown {
            name: "Color",
            param: Some("f.Color"),
            order: &["R", "U"],
            labels: |c| if c.id == "token" { vec![] } else { vec!["R".into()] },
        };
        let s = statistics(&[bolt, ring, token], &[colors], &["common", "uncommon", "rare", "mythic"], &["nonfoil", "foil", "etched"]);

        let rarity: Vec<&str> = s.breakdowns[0].rows.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(rarity, ["common", "mythic", "special"], "the game's order, then the rest");
        assert_eq!(*row(&s, "rarity", "common"), BreakdownRow { key: "common".into(), cards: 4, unique: 1, value: 5.5, paid: 2.75 });
        assert_eq!((row(&s, "finish", "foil").cards, row(&s, "finish", "foil").value, row(&s, "finish", "nonfoil").cards), (1, 4.0, 6));
        assert_eq!((row(&s, "condition", "LP").cards, row(&s, "condition", "NM").unique), (1, 3));
        assert_eq!((row(&s, "language", "ja").cards, row(&s, "language", "en").cards), (1, 6));
        let years: Vec<&str> = s.breakdowns.iter().find(|t| t.id == "year").unwrap().rows.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(years, ["2023", "2010"], "newest first");
        assert_eq!((row(&s, "Color", "R").cards, row(&s, "Color", "R").unique), (5, 2));
        assert_eq!(s.breakdowns.last().unwrap().param.as_deref(), Some("f.Color"));

        // Bands by one copy's best price: Bolt $4 ($1 to $5), Ring $150, Token none.
        let band = |label: &str| s.bands.iter().find(|b| b.label == label).unwrap().clone();
        assert_eq!(
            (band("$1 to $5").cards, band("$1 to $5").value, band("$1 to $5").min, band("$1 to $5").max),
            (4, 5.5, Some(1.0), Some(5.0))
        );
        assert_eq!((band("$100 and up").unique, band("$100 and up").share), (1, 0.965), "150 of 155.5");
        assert_eq!((band("Under $1").cards, band("Under $1").min), (0, None));
        assert_eq!(band("No price").cards, 2);

        let months: Vec<(&str, i64, f64)> = s.by_month.iter().map(|m| (m.month.as_str(), m.cards, m.paid)).collect();
        assert_eq!(months, [("2024-01", 2, 0.0), ("2024-03", 4, 100.75)]);
        assert_eq!(s.undated, 1);
    }

    #[test]
    fn empty_collection() {
        let s = statistics(&[], &[], &[], &[]);
        assert!(s.breakdowns.iter().all(|t| t.rows.is_empty()));
        assert_eq!(s.bands.len(), 5, "no No price band without unpriced cards");
        assert!(s.bands.iter().all(|b| b.share == 0.0));
    }
}
