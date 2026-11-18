//! The watchlist and price alerts (see `tcgbinder_core::watchlist`).

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{Value, json};

use tcgbinder_core::collection::{Card, round_cents};
use tcgbinder_core::dates;
use tcgbinder_core::service::GameService;
use tcgbinder_core::watchlist::{Rule, Watch, WatchChange};

use crate::views::view;
use crate::{Api, Reply, Result, bad, decode, internal, not_found};

/// A watch as the API answers it: the watch, its `key`, the current `price`, and
/// the `change` from the baseline in dollars and `changePercent`.
pub(crate) fn watch_json(w: &Watch) -> Value {
    let price = w.price();
    let change = price.filter(|_| w.baseline > 0.0).map(|p| p - w.baseline);
    json!({
        "key": w.key(), "card": view(&w.card), "finish": w.finish, "added": w.added, "baseline": w.baseline,
        "baseline_date": w.baseline_date, "rule": w.rule, "triggered": w.triggered, "price": price,
        "change": change.map(round_cents), "changePercent": change.map(|c| (c / w.baseline * 1000.0).round() / 10.0),
    })
}

/// The triggered watches, for Home.
pub(crate) fn alerts(g: &GameService) -> Vec<Value> {
    g.watchlist.all().iter().filter(|w| w.triggered.is_some()).map(watch_json).collect()
}

impl Api {
    fn watch_key(params: &HashMap<String, String>) -> &str {
        params.get("key").map_or("", String::as_str)
    }

    pub(crate) fn watchlist(&self, g: &GameService) -> Reply {
        Reply::Json(json!(g.watchlist.all().iter().map(watch_json).collect::<Vec<_>>()))
    }

    /// Starts watching a catalog card: `{"card", "finish", "rule": {"kind", "value"}}`.
    pub(crate) fn add_watch(&self, g: &GameService, body: &[u8]) -> Result {
        #[derive(Deserialize)]
        struct Req {
            card: Card,
            finish: String,
            #[serde(default)]
            rule: Rule,
        }
        let req: Req = decode(body)?;
        let w = g.watchlist.watch(req.card, &req.finish, req.rule, &dates::today_string()).map_err(|e| bad(e.to_string()))?;
        tracing::info!(game = g.game.id(), card = w.card.name, finish = w.finish, "watching");
        Ok(Reply::Json(watch_json(&w)))
    }

    /// Changes a watch: `{"rule"}`, or `{"action": "dismiss"}` or `{"action": "reset"}`.
    pub(crate) fn change_watch(&self, g: &GameService, params: &HashMap<String, String>, body: &[u8]) -> Result {
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Req {
            rule: Option<Rule>,
            action: String,
        }
        let req: Req = decode(body)?;
        let change = match (req.rule, req.action.as_str()) {
            (Some(r), "") => WatchChange::Rule(r),
            (None, "dismiss") => WatchChange::Dismiss,
            (None, "reset") => WatchChange::Reset,
            _ => return Err(bad("send a rule, or an action: dismiss or reset")),
        };
        let key = Self::watch_key(params);
        if g.watchlist.get(key).is_none() {
            return Err(not_found("that card isn't on the watchlist"));
        }
        let w = g.watchlist.change(key, change, &dates::today_string()).map_err(|e| bad(e.to_string()))?;
        Ok(Reply::Json(watch_json(&w)))
    }

    pub(crate) fn delete_watch(&self, g: &GameService, params: &HashMap<String, String>) -> Result {
        let key = Self::watch_key(params);
        let w = g.watchlist.get(key).ok_or_else(|| not_found("that card isn't on the watchlist"))?;
        g.watchlist.remove(key).map_err(internal)?;
        tracing::info!(game = g.game.id(), card = w.card.name, "stopped watching");
        Ok(Reply::Json(json!({ "deleted": key })))
    }
}
