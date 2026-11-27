//! The game's catalog: searching it to add cards, and scanning a card from a photo.

use crate::views::{view, views};
use crate::{Api, Error, Reply, Result, bad, not_found, upstream};
use http::StatusCode;
use serde_json::json;
use std::collections::HashMap;
use tcgbinder_core::collection::{Card, card_key};
use tcgbinder_core::scan::Reading;
use tcgbinder_core::service::GameService;

/// The copies owned of each catalog card.
fn owned(g: &GameService, cards: &[Card]) -> Vec<i64> {
    cards.iter().map(|c| g.store.get(&card_key(c)).map_or(0, |o| o.copy_count())).collect()
}

impl Api {
    pub(crate) fn search(&self, g: &GameService, params: &HashMap<String, String>) -> Result {
        let get = |k: &str| params.get(k).map_or("", String::as_str);
        // rules=1 searches the rules text instead of names, one result per card.
        let page = if get("rules") == "1" {
            g.game.search_rules(get("text"), get("next"))
        } else {
            g.game.search(get("text"), get("set"), get("next"))
        }
        .map_err(upstream)?;
        // Cards already owned say so.
        Ok(Reply::Json(json!({ "cards": views(&page.cards), "owned": owned(g, &page.cards), "total": page.total, "next": page.next_page })))
    }

    /// A catalog printing you may not own, by its ID (a card's key): the card, the
    /// game's details about it, its catalog link, and the other printings you own.
    pub(crate) fn catalog_card(&self, g: &GameService, id: &str) -> Result {
        let c = g.game.card(id).map_err(upstream)?.ok_or_else(|| not_found("no such card in the catalog"))?;
        let mut others = g.store.cards_where(|o| o.name == c.name && o.id != c.id);
        others.sort_by(|a, b| b.released_at.cmp(&a.released_at));
        let (label, url) = g.game.card_link(&c).unwrap_or_default();
        Ok(Reply::Json(json!({
            "card": view(&c), "details": g.game.details(&c), "link": { "label": label, "url": url }, "others": views(&others),
        })))
    }

    /// Every set in the catalog, newest first: what the Add dialog's set menu offers.
    pub(crate) fn catalog_sets(&self, g: &GameService) -> Result {
        let sets = g.sets().map_err(upstream)?;
        let mut all: Vec<_> = sets.values().collect();
        all.sort_by(|a, b| b.released_at.cmp(&a.released_at).then_with(|| a.name.cmp(&b.name)));
        let all: Vec<_> = all.iter().map(|s| json!({ "code": s.code, "name": s.name, "released": s.released_at })).collect();
        Ok(Reply::Json(json!({ "sets": all })))
    }

    pub(crate) fn scanning(&self, g: &GameService) -> bool {
        self.read_image.is_some() && g.game.scanner().is_some()
    }

    pub(crate) fn read_photo(&self, g: &GameService, body: &[u8]) -> Result<Reading> {
        let read = self.read_image.filter(|_| self.scanning(g)).ok_or_else(|| Error {
            status: StatusCode::NOT_IMPLEMENTED,
            message: "card scanning isn't available here (it needs macOS or iOS)".into(),
        })?;
        read(body).map_err(|e| bad(format!("{e:#}")))
    }

    /// Reads a photo of a card and looks it up: the printings it could be, best first.
    pub(crate) fn scan(&self, g: &GameService, body: &[u8]) -> Result {
        let reading = self.read_photo(g, body)?;
        let scanner = g.game.scanner().expect("checked by read_photo");
        let res = scanner.identify(&reading).map_err(upstream)?;
        tracing::info!(game = g.game.id(), read = res.read, exact = res.exact, candidates = res.candidates.len(), "card scanned");
        Ok(Reply::Json(json!({
            "read": res.read, "name": res.name, "exact": res.exact, "note": res.note,
            "candidates": views(&res.candidates), "owned": owned(g, &res.candidates), "text": reading.text(),
        })))
    }

    /// Reads one camera frame: what it says about the card, and whether that is enough
    /// to look it up. The camera captures once two frames in a row agree.
    pub(crate) fn scan_frame(&self, g: &GameService, body: &[u8]) -> Result {
        let reading = self.read_photo(g, body)?;
        let (key, complete) = g.game.scanner().expect("checked by read_photo").scan_key(&reading);
        Ok(Reply::Json(json!({ "key": key, "complete": complete, "lines": reading.lines.len() })))
    }
}
