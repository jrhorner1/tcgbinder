//! How cards, purchases and chart points look in API responses.

use chrono::NaiveDate;
use serde::Serialize;

use tcgbinder_core::collection::{Card, TimePoint, card_key, round_cents};
use tcgbinder_core::dates;

/// A card as the front end sees it: the card as stored, plus its key, totals, and its
/// purchases as a flat list (finish, condition, quantity, price paid, date), as the
/// "Your Copies" rows show them.
#[derive(Serialize)]
pub struct CardView<'a> {
    #[serde(flatten)]
    pub card: &'a Card,
    pub key: String,
    /// Copies owned.
    pub copy_count: i64,
    /// The market price of one copy in its most valuable owned finish.
    pub each: f64,
    /// The market value of every copy.
    pub market_value: f64,
    /// The price paid for every copy with a known price.
    pub price_paid: f64,
    pub purchases: Vec<PurchaseView>,
}

/// One purchase of a card in a finish and condition.
#[derive(Serialize)]
pub struct PurchaseView {
    pub finish: String,
    pub condition: &'static str,
    pub quantity: i64,
    /// Per copy.
    pub price_paid: f64,
    pub acquired_date: String,
    /// The market price of one copy in this finish (0 if unknown).
    pub market: f64,
}

pub fn view(c: &Card) -> CardView<'_> {
    let purchases = c
        .purchase_rows()
        .into_iter()
        .map(|r| PurchaseView {
            market: c.market_price(&r.finish).unwrap_or(0.0),
            finish: r.finish,
            condition: r.condition.code(),
            quantity: r.purchase.quantity,
            price_paid: r.purchase.price_paid,
            acquired_date: r.purchase.acquired_date,
        })
        .collect();
    CardView {
        card: c,
        key: card_key(c),
        copy_count: c.copy_count(),
        each: round_cents(c.best_price().unwrap_or(0.0)),
        market_value: round_cents(c.market_value().0),
        price_paid: round_cents(c.price_paid().0),
        purchases,
    }
}

pub fn views(cards: &[Card]) -> Vec<CardView<'_>> {
    cards.iter().map(view).collect()
}

/// A point on a chart line.
#[derive(Serialize)]
pub struct Point {
    pub date: String,
    pub value: f64,
}

pub fn points(ps: &[TimePoint]) -> Vec<Point> {
    ps.iter().map(|p| Point { date: dates::format(p.date), value: round_cents(p.value) }).collect()
}

/// A date as `YYYY-MM-DD`, or "".
pub fn date_or_empty(d: Option<NaiveDate>) -> String {
    d.map(dates::format).unwrap_or_default()
}
