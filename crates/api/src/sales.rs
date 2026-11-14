//! Sold cards and earnings (see `tcgbinder_core::sales`).

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{Value, json};

use tcgbinder_core::collection::{Condition, Purchase, round_cents};
use tcgbinder_core::dates;
use tcgbinder_core::sales::{Sale, SalesTotals, sales_totals};
use tcgbinder_core::service::GameService;

use crate::{Api, Reply, Result, bad, check_date, decode};

/// `POST /card/sell`: the purchase to sell from (finish, condition, price paid,
/// acquired date, note) and the sale.
#[derive(Deserialize)]
#[serde(default)]
struct SellForm {
    finish: String,
    condition: String,
    price_paid: f64,
    acquired_date: String,
    note: String,
    quantity: i64,
    sale_each: f64,
    date: String,
    fees: f64,
    sale_note: String,
}

impl Default for SellForm {
    fn default() -> SellForm {
        SellForm {
            finish: String::new(),
            condition: String::new(),
            price_paid: 0.0,
            acquired_date: String::new(),
            note: String::new(),
            quantity: 1,
            sale_each: 0.0,
            date: String::new(),
            fees: 0.0,
            sale_note: String::new(),
        }
    }
}

fn totals_json(t: &SalesTotals) -> Value {
    json!({
        "sales": t.sales, "copies": t.copies, "proceeds": t.proceeds, "fees": t.fees, "cost": t.cost, "gain": t.gain,
        "unknownCostCopies": t.unknown_cost_copies,
    })
}

/// The gain realised by sales this calendar year (sales of known cost only).
pub(crate) fn realised_this_year(g: &GameService) -> f64 {
    let year = dates::today_string()[..4].to_string();
    let sales = g.sales.all();
    sales_totals(sales.iter().filter(|s| s.date.starts_with(&year))).0.gain
}

impl Api {
    /// Sells copies from one of a card's purchases.
    pub(crate) fn sell(&self, g: &GameService, key: &str, body: &[u8]) -> Result {
        let f: SellForm = decode(body)?;
        let date = if f.date.trim().is_empty() { dates::today_string() } else { f.date.trim().to_string() };
        if date.is_empty() || dates::parse(&date).is_none() {
            return Err(bad("the sale date must look like 2024-03-15"));
        }
        check_date(&f.acquired_date)?;
        let sale = Sale {
            date,
            finish: f.finish.trim().to_lowercase(),
            condition: Condition::parse(&f.condition),
            quantity: f.quantity,
            sale_each: f.sale_each,
            fees: f.fees,
            note: f.sale_note.trim().to_string(),
            ..Sale::default()
        };
        sale.validate().map_err(|e| bad(e.to_string()))?;
        let purchase = Purchase { price_paid: f.price_paid, acquired_date: f.acquired_date, note: f.note, ..Purchase::default() };
        let sale = g.sell(key, &purchase, sale).map_err(|e| bad(format!("{e:#}")))?;
        Ok(Reply::Json(json!({ "sale": sale, "gain": sale.gain().map(round_cents) })))
    }

    /// The sales, newest first, with totals and totals by month: all of them, or
    /// one year's (`year=2026`). `years` lists the years with sales.
    pub(crate) fn sales(&self, g: &GameService, params: &HashMap<String, String>) -> Result {
        let year = params.get("year").map(|y| y.trim()).unwrap_or("");
        if !year.is_empty() && (year.len() != 4 || !year.bytes().all(|b| b.is_ascii_digit())) {
            return Err(bad("year must look like 2026"));
        }
        let all = g.sales.all();
        let mut years: Vec<&str> = all.iter().filter_map(|s| s.date.get(..4)).collect();
        years.dedup();
        let shown: Vec<&Sale> = all.iter().filter(|s| s.date.starts_with(year)).collect();
        let (totals, months) = sales_totals(shown.iter().copied());
        let sales: Vec<Value> = shown
            .iter()
            .map(|s| {
                let mut v = json!(s);
                v["proceeds"] = json!(round_cents(s.proceeds()));
                v["gain"] = json!(s.gain().map(round_cents));
                v
            })
            .collect();
        Ok(Reply::Json(json!({
            "year": year, "years": years, "totals": totals_json(&totals),
            "months": months.iter().map(|(m, t)| { let mut v = totals_json(t); v["month"] = json!(m); v }).collect::<Vec<_>>(),
            "sales": sales,
        })))
    }

    /// Deletes a sale; `restore=1` puts its copies back in the collection.
    pub(crate) fn delete_sale(&self, g: &GameService, params: &HashMap<String, String>) -> Result {
        let id = params.get("id").map_or("", String::as_str);
        let restore = params.get("restore").is_some_and(|v| v == "1" || v == "true");
        let sale = g.delete_sale(id, restore).map_err(|e| bad(format!("{e:#}")))?;
        Ok(Reply::Json(json!({ "deleted": sale.id, "restored": restore })))
    }
}
