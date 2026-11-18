//! Sealed product (see `tcgbinder_core::sealed`).

use std::collections::HashMap;

use serde_json::{Value, json};

use tcgbinder_core::dates;
use tcgbinder_core::sealed::{SealedItem, sealed_totals};
use tcgbinder_core::service::GameService;

use crate::{Api, Reply, Result, bad, check_date, decode, internal, not_found, upstream};

/// An item as the API answers it: the item, with `valueStale` (no value, or one more
/// than 90 days old) and `gain` (value less price paid, every copy; null unless both
/// are set).
fn item_json(i: &SealedItem, today: &str) -> Value {
    let mut v = json!(i);
    v["valueStale"] = json!(i.value_stale(today));
    v["gain"] =
        json!((i.value > 0.0 && i.price_paid > 0.0).then(|| ((i.value - i.price_paid) * i.quantity as f64 * 100.0).round() / 100.0));
    v
}

/// The sealed product's value, for Home: `{"value", "items", "unvalued"}`, or null
/// when there is none.
pub(crate) fn home_summary(g: &GameService) -> Value {
    let items = g.sealed.all();
    if items.is_empty() {
        return Value::Null;
    }
    let t = sealed_totals(&items, &dates::today_string());
    json!({ "value": t.value, "items": t.items, "unvalued": t.unvalued })
}

/// Checks the dates a request sent.
fn check_dates(i: &SealedItem) -> Result<()> {
    check_date(&i.acquired_date)?;
    if !i.value_date.is_empty() && dates::parse(&i.value_date).is_none() {
        return Err(bad("the value's date must look like 2024-03-15"));
    }
    Ok(())
}

impl Api {
    /// Every item, with totals (`items`, `paid`, `value`, `unvalued`, `stale`).
    pub(crate) fn sealed(&self, g: &GameService) -> Reply {
        let today = dates::today_string();
        let items = g.sealed.all();
        Reply::Json(json!({
            "items": items.iter().map(|i| item_json(i, &today)).collect::<Vec<_>>(),
            "totals": sealed_totals(&items, &today),
        }))
    }

    /// Adds an item. A value without a date is dated today.
    pub(crate) fn add_sealed(&self, g: &GameService, body: &[u8]) -> Result {
        let mut item: SealedItem = decode(body)?;
        check_dates(&item)?;
        if item.value > 0.0 && item.value_date.is_empty() {
            item.value_date = dates::today_string();
        }
        let item = g.sealed.add(item).map_err(|e| bad(e.to_string()))?;
        tracing::info!(game = g.game.id(), product = item.name, quantity = item.quantity, "sealed product added");
        Ok(Reply::Json(item_json(&item, &dates::today_string())))
    }

    /// Replaces an item's fields. A changed value without a new date is dated today.
    pub(crate) fn update_sealed(&self, g: &GameService, params: &HashMap<String, String>, body: &[u8]) -> Result {
        let id = params.get("id").map_or("", String::as_str);
        let old = g.sealed.get(id).ok_or_else(|| not_found(format!("there is no sealed item {id:?}")))?;
        let mut item: SealedItem = decode(body)?;
        item.id = old.id.clone();
        check_dates(&item)?;
        if item.value > 0.0 && item.value != old.value && item.value_date == old.value_date {
            item.value_date = dates::today_string();
        }
        let item = g.sealed.save(item).map_err(|e| bad(e.to_string()))?;
        Ok(Reply::Json(item_json(&item, &dates::today_string())))
    }

    pub(crate) fn delete_sealed(&self, g: &GameService, params: &HashMap<String, String>) -> Result {
        let id = params.get("id").map_or("", String::as_str);
        let item = g.sealed.get(id).ok_or_else(|| not_found(format!("there is no sealed item {id:?}")))?;
        g.sealed.remove(id).map_err(internal)?;
        tracing::info!(game = g.game.id(), product = item.name, "sealed product removed");
        Ok(Reply::Json(json!({ "deleted": item.name })))
    }

    /// The catalog's sealed products for a set.
    pub(crate) fn sealed_products(&self, g: &GameService, params: &HashMap<String, String>) -> Result {
        let set = params.get("set").map_or("", |s| s.trim());
        if set.is_empty() {
            return Err(bad("choose a set"));
        }
        let products = g.game.sealed_products(set).map_err(upstream)?;
        Ok(Reply::Json(json!({ "products": products })))
    }
}
