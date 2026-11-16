//! Lots: cards bought together for one price (see `tcgbinder_core::lots`).

use serde::Deserialize;
use serde_json::{Value, json};

use tcgbinder_core::collection::{Card, Copies, CopyPick, LOT_PREFIX, Purchase, lot_tag, normalize_tags, round_cents};
use tcgbinder_core::lots::{Lot, LotSplit};
use tcgbinder_core::service::GameService;

use crate::views::views;
use crate::{Api, Reply, Result, bad, check_date, decode, internal, not_found, undo};

/// A lot as the API answers it: the lot, with `entered` (copies so far that count
/// toward the even split), `copies`, `cards` (printings), `expected` (the divisor, null
/// when counted as entered), `priceEach` (the even price, while open or for an even
/// split) and `marketValue` (what its cards are worth now).
pub(crate) fn summary(g: &GameService, lot: &Lot) -> Value {
    let copies = g.lot_copies(lot);
    let count: i64 = copies.iter().map(|c| c.quantity).sum();
    let market: f64 = copies.iter().map(|c| c.market.unwrap_or(0.0) * c.quantity as f64).sum();
    let printings = copies.iter().map(|c| c.card_id.as_str()).collect::<std::collections::HashSet<_>>().len();
    let mut v = serde_json::to_value(lot).unwrap_or_default();
    v["entered"] = json!(lot.entered(&copies));
    v["copies"] = json!(count);
    v["cards"] = json!(printings);
    v["expected"] = json!(lot.expected());
    v["priceEach"] = json!((lot.open || lot.split == LotSplit::Even).then(|| lot.even_price(&copies)).flatten());
    v["marketValue"] = json!(round_cents(market));
    v
}

/// Puts every purchase of `cards` in `lot`, at the lot's date (the price comes from
/// repricing the lot once they're saved).
pub(crate) fn put_in_lot(cards: &mut [Card], lot: &Lot) {
    for c in cards {
        let mut copies = Copies::default();
        for (finish, cond, p) in c.copies.each() {
            let mut tags: Vec<String> = p.tags.iter().filter(|t| !t.starts_with(LOT_PREFIX)).cloned().collect();
            tags.push(lot_tag(&lot.id));
            let tags = normalize_tags(&tags).unwrap_or(tags); // already normal; a lot's tag doesn't count toward the limit
            let p = Purchase { tags, price_paid: 0.0, acquired_date: lot.acquired_date.clone(), ..p.clone() };
            copies.add(finish, cond, p);
        }
        c.copies = copies;
    }
}

/// The lot fields a request may set.
const EDITABLE: &[&str] = &["name", "acquired_date", "total_paid", "count", "note", "split", "open"];

/// `lot` with the editable fields in `change` replaced.
fn with_changes(lot: &Lot, change: &Value) -> Result<Lot> {
    let Value::Object(change) = change else { return Err(bad("send the lot's fields as a JSON object")) };
    let mut v = serde_json::to_value(lot).map_err(|e| internal(e.into()))?;
    for k in EDITABLE {
        if let Some(x) = change.get(*k) {
            v[*k] = x.clone();
        }
    }
    let mut next: Lot = serde_json::from_value(v).map_err(|e| bad(format!("{e}")))?;
    next.name = next.name.trim().to_string();
    next.id = lot.id.clone();
    next.created = lot.created.clone();
    check_date(&next.acquired_date)?;
    next.validate().map_err(|e| bad(e.to_string()))?;
    Ok(next)
}

impl Api {
    fn find_lot(g: &GameService, id: &str) -> Result<Lot> {
        g.lots.get(id).ok_or_else(|| not_found(format!("there is no lot {id:?}")))
    }

    /// A lot that cards can be added to: it must exist and still be open.
    pub(crate) fn open_lot(&self, g: &GameService, id: &str) -> Result<Lot> {
        let lot = Self::find_lot(g, id)?;
        if !lot.open {
            return Err(bad(format!("{:?} is finished; choose Continue adding on its page to add more", lot.name)));
        }
        Ok(lot)
    }

    pub(crate) fn list_lots(&self, g: &GameService) -> Reply {
        Reply::Json(json!(g.lots.all().iter().map(|l| summary(g, l)).collect::<Vec<_>>()))
    }

    /// One lot and its cards.
    pub(crate) fn lot(&self, g: &GameService, id: &str) -> Result {
        let lot = Self::find_lot(g, id)?;
        let cards = g.store.cards_with_tag(&lot_tag(id));
        Ok(Reply::Json(json!({ "lot": summary(g, &lot), "cards": views(&cards) })))
    }

    pub(crate) fn create_lot(&self, g: &GameService, body: &[u8]) -> Result {
        let change: Value = decode(body)?;
        let lot = with_changes(&Lot::default(), &change)?;
        let lot = g.lots.add(lot).map_err(|e| bad(e.to_string()))?;
        tracing::info!(game = g.game.id(), lot = lot.name, "lot created");
        Ok(Reply::Json(summary(g, &lot)))
    }

    /// Changes a lot. Changing its total, count, date or split, finishing it, or
    /// `"recalculate": true` reprices its cards. Finishing a lot whose entered count
    /// differs from its count says so (`short`); finishing a value split gives the
    /// outcome (`outcome`: paid, market value and the percentage of it paid).
    pub(crate) fn update_lot(&self, g: &GameService, id: &str, body: &[u8]) -> Result {
        let old = Self::find_lot(g, id)?;
        let change: Value = decode(body)?;
        let lot = with_changes(&old, &change)?;
        let recalculate = change.get("recalculate").and_then(Value::as_bool).unwrap_or(false);
        let reprice = recalculate
            || lot.total_paid != old.total_paid
            || lot.count != old.count
            || lot.acquired_date != old.acquired_date
            || lot.split != old.split
            || lot.open != old.open;
        if !reprice {
            g.lots.save(lot.clone()).map_err(|e| bad(e.to_string()))?;
            return Ok(Reply::Json(json!({ "lot": summary(g, &lot) })));
        }
        let (lot, prices) = g.save_lot(lot, recalculate).map_err(internal)?;
        let sum = summary(g, &lot);
        let mut answer = json!({ "lot": sum, "note": prices.note });
        let finished = old.open && !lot.open;
        if finished && let Some(expected) = lot.expected() {
            let entered = sum["entered"].as_i64().unwrap_or(0);
            if entered != expected {
                answer["short"] = json!({ "entered": entered, "expected": expected });
            }
        }
        if !lot.open && lot.split == LotSplit::ByValue {
            let market = sum["marketValue"].as_f64().unwrap_or(0.0);
            let percent = (market > 0.0).then(|| (lot.total_paid / market * 1000.0).round() / 10.0);
            answer["outcome"] = json!({ "paid": lot.total_paid, "marketValue": market, "percent": percent });
        }
        tracing::info!(game = g.game.id(), lot = lot.name, open = lot.open, "lot changed");
        Ok(Reply::Json(answer))
    }

    /// Puts cards the collection holds into a lot (after a backup):
    /// `{"cards": [{"key", "quantity"}]}`, `quantity` optional (every copy not in the
    /// lot). The lot, and any lot the copies left, are repriced. Returns `lot`,
    /// `moved` and `undo`.
    pub(crate) fn put_cards_in_lot(&self, g: &GameService, id: &str, body: &[u8]) -> Result {
        #[derive(Deserialize)]
        struct Pick {
            key: String,
            #[serde(default)]
            quantity: Option<i64>,
        }
        #[derive(Deserialize)]
        struct Req {
            cards: Vec<Pick>,
        }
        let lot = Self::find_lot(g, id)?;
        let req: Req = decode(body)?;
        if req.cards.is_empty() {
            return Err(bad("choose at least one card"));
        }
        let picks: Vec<CopyPick> = req.cards.into_iter().map(|p| CopyPick { key: p.key, quantity: p.quantity }).collect();
        let backup = g.store.backup("bulk-edit").map_err(internal)?;
        let date = lot.acquired_date.clone();
        let (moved, replaced) =
            g.store.tag_copies(&lot_tag(&lot.id), &picks, |p| p.acquired_date = date.clone()).map_err(|e| bad(e.to_string()))?;
        let left: Vec<&str> = replaced.iter().filter_map(|t| t.strip_prefix(LOT_PREFIX)).collect();
        g.reprice_lot(&lot).map_err(internal)?;
        for other in left.iter().filter_map(|l| g.lots.get(l)) {
            g.reprice_lot(&other).map_err(internal)?;
        }
        tracing::info!(game = g.game.id(), lot = lot.name, moved, "cards put in lot");
        Ok(Reply::Json(json!({ "lot": summary(g, &lot), "moved": moved, "undo": undo(g, &backup) })))
    }

    /// Takes a card out of a lot (`key=`): its copies stay in the collection at the
    /// price the lot gave them, and the lot's other cards are repriced. Returns `lot`
    /// and `freed`.
    pub(crate) fn remove_card_from_lot(&self, g: &GameService, id: &str, key: &str) -> Result {
        let lot = Self::find_lot(g, id)?;
        let freed = g.store.untag_card(&lot_tag(&lot.id), key).map_err(|e| bad(e.to_string()))?;
        g.reprice_lot(&lot).map_err(internal)?;
        tracing::info!(game = g.game.id(), lot = lot.name, freed, "card taken out of lot");
        Ok(Reply::Json(json!({ "lot": summary(g, &lot), "freed": freed })))
    }

    /// Deletes a lot: `cards=keep` leaves its copies with their prices, `cards=remove`
    /// deletes them (after a backup).
    pub(crate) fn delete_lot(&self, g: &GameService, id: &str, cards: &str) -> Result {
        let lot = Self::find_lot(g, id)?;
        let remove = match cards {
            "keep" => false,
            "remove" => true,
            _ => return Err(bad("say what happens to the lot's cards: cards=keep or cards=remove")),
        };
        let backup = if remove { Some(g.store.backup("bulk-delete").map_err(internal)?) } else { None };
        let tag = lot_tag(id);
        if remove { g.store.remove_tagged(&tag) } else { g.store.untag_all(&tag) }.map_err(internal)?;
        g.lots.remove(id).map_err(internal)?;
        tracing::info!(game = g.game.id(), lot = lot.name, remove, "lot deleted");
        let mut answer = json!({ "deleted": lot.name });
        if let Some(b) = backup {
            answer["undo"] = undo(g, &b);
        }
        Ok(Reply::Json(answer))
    }
}
