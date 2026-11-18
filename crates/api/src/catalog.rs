//! The game's catalog: searching it to add cards, and scanning a card from a photo.

use crate::views::views;
use crate::{Api, Error, Reply, Result, bad, upstream};
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
        let page = g.game.search(get("text"), get("set"), get("next")).map_err(upstream)?;
        // Cards already owned say so.
        Ok(Reply::Json(json!({ "cards": views(&page.cards), "owned": owned(g, &page.cards), "total": page.total, "next": page.next_page })))
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
