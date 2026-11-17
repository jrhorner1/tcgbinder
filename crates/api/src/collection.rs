//! The collection's cards: the filtered and sorted list, one card, adding, editing,
//! deleting and moving a card to another printing, and bulk edits.

use crate::views::{self, view, views};
use crate::{Api, Reply, Result, bad, check_date, decode, internal, lots, not_found, tags_from, undo, upstream};
use serde::Deserialize;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use tcgbinder_core::collection::{
    BulkChange, Card, Condition, Query, SortKey, card_has_tag, card_key, clean_user_tag, is_any, is_system_tag, lot_of, lot_tag,
    round_cents, sort_cards, split_tags, unique_cards, with_printing,
};
use tcgbinder_core::service::GameService;
use tcgbinder_core::{Purchase, dates};

/// One purchase sent by the front end: copies in a finish and condition.
#[derive(Deserialize, Default)]
#[serde(default)]
struct PurchaseForm {
    quantity: i64,
    finish: String,
    condition: String,
    /// Per copy.
    price_paid: f64,
    acquired_date: String,
    /// The lot the copies belong to; its total sets their price.
    lot: String,
    /// The owner's tags on these copies.
    tags: Vec<String>,
}

impl PurchaseForm {
    fn validate(&self, c: &Card) -> Result<()> {
        if !self.finish.is_empty() && !c.finishes.is_empty() && !c.finishes.contains(&self.finish) {
            return Err(bad(format!("this printing doesn't come in {:?}", self.finish)));
        }
        if self.quantity < 0 {
            return Err(bad("quantity must be 0 or more"));
        }
        if self.price_paid < 0.0 || !self.price_paid.is_finite() {
            return Err(bad("price paid can't be negative"));
        }
        views::stored_tags(&self.tags, &self.lot).map_err(|e| bad(e.to_string()))?;
        check_date(&self.acquired_date)
    }

    /// Adds the purchase to `c` (in the printing's first finish if none is given).
    fn add(&self, c: &mut Card) {
        let finish = if self.finish.is_empty() { c.finishes.first().cloned().unwrap_or_default() } else { self.finish.clone() };
        let p = Purchase {
            quantity: self.quantity,
            price_paid: self.price_paid,
            acquired_date: self.acquired_date.clone(),
            tags: views::stored_tags(&self.tags, &self.lot).unwrap_or_default(), // checked by validate
        };
        c.add_purchase(&finish, Condition::parse(&self.condition), p);
    }
}

/// The lots a card has copies in.
fn lots_of(c: &Card) -> Vec<String> {
    c.copies.each().filter_map(|(_, _, p)| lot_of(&p.tags).map(str::to_string)).collect()
}

/// Reprices each lot named (once), so its cards' shares follow copies added, removed or
/// moved. Lots that no longer exist are skipped.
fn reprice_lots(g: &GameService, ids: impl IntoIterator<Item = String>) -> Result<()> {
    let mut seen = HashSet::new();
    for id in ids {
        if seen.insert(id.clone())
            && let Some(lot) = g.lots.get(&id)
        {
            g.reprice_lot(&lot).map_err(internal)?;
        }
    }
    Ok(())
}

impl Api {
    /// The filtered cards, sorted, with counts and money totals. Filters: q, set,
    /// rarity, finish, condition, language, card_type, lot (an ID, or "none" for copies
    /// in no lot), price_min,
    /// price_max, acquired_from, acquired_to, min_copies, no_price_paid, unpriced,
    /// tags, tags_mode (all or any), tags_not, untagged, and one per game filter,
    /// named f.<filter name>. Sorting: sort (a [`SortKey`] name)
    /// and dir (asc or desc); without sort, the Card order setting.
    pub(crate) fn cards(&self, g: &GameService, params: &HashMap<String, String>) -> Result {
        let (filtered, key, descending) = self.filtered_cards(g, params)?;
        let card_count: i64 = filtered.iter().map(Card::copy_count).sum();
        let sets: HashSet<String> = filtered.iter().map(|c| c.set.to_uppercase()).collect();
        let market: f64 = filtered.iter().map(|c| c.market_value().0).sum();
        let paid: f64 = filtered.iter().map(|c| c.price_paid().0).sum();
        let sort_name = SortKey::ALL.iter().find(|(_, k)| *k == key).map_or("name", |(n, _)| *n);
        Ok(Reply::Json(json!({
            "total": g.store.len(), "matched": filtered.len(), "cardCount": card_count,
            "uniqueCards": unique_cards(&filtered), "sets": sets.len(),
            "marketValue": round_cents(market), "pricePaid": round_cents(paid),
            "sort": sort_name, "dir": if descending { "desc" } else { "asc" },
            "cards": views(&filtered),
        })))
    }

    /// The cards `GET /cards` would list for these parameters, and the order used.
    pub(crate) fn filtered_cards(&self, g: &GameService, params: &HashMap<String, String>) -> Result<(Vec<Card>, SortKey, bool)> {
        let filters = g.game.filters();
        let get = |k: &str| params.get(k).map(|v| v.trim().to_string()).unwrap_or_default();
        let flag = |k: &str| matches!(get(k).as_str(), "1" | "true" | "yes");
        let number = |k: &str| -> Result<Option<f64>> {
            let v = get(k);
            if v.is_empty() {
                return Ok(None);
            }
            v.parse::<f64>()
                .ok()
                .filter(|n| n.is_finite() && *n >= 0.0)
                .map(Some)
                .ok_or_else(|| bad(format!("{k} must be a number, such as 2.50")))
        };
        let date = |k: &str| -> Result<String> {
            let v = get(k);
            if v.is_empty() || dates::parse(&v).is_some() { Ok(v) } else { Err(bad(format!("{k} must be a date like 2024-03-15"))) }
        };
        // `lot=`: a lot's ID, or "none" for cards with copies in no lot.
        let no_lot = get("lot") == "none";
        let lot = Some(get("lot")).filter(|l| !l.is_empty() && !no_lot).map(|l| lot_tag(&l));
        let q = Query {
            text: get("q"),
            set: get("set"),
            rarity: get("rarity"),
            finish: get("finish"),
            condition: get("condition"),
            language: get("language"),
            card_type: get("card_type"),
            price_min: number("price_min")?,
            price_max: number("price_max")?,
            acquired_from: date("acquired_from")?,
            acquired_to: date("acquired_to")?,
            min_copies: number("min_copies")?.map(|n| n as i64),
            no_price_paid: flag("no_price_paid"),
            unpriced: flag("unpriced"),
            matches: Some(Box::new(move |c: &Card| {
                lot.as_ref().is_none_or(|tag| card_has_tag(c, tag))
                    && (!no_lot || c.copies.each().any(|(_, _, p)| lot_of(&p.tags).is_none()))
                    && filters.iter().all(|f| params.get(&format!("f.{}", f.name)).is_none_or(|opt| is_any(opt) || (f.matches)(c, opt)))
            })),
            tags: split_tags(&get("tags")),
            tags_any: match get("tags_mode").as_str() {
                "" | "all" => false,
                "any" => true,
                _ => return Err(bad("tags_mode must be all or any")),
            },
            tags_not: split_tags(&get("tags_not")),
            untagged: flag("untagged"),
        };
        let (key, descending) = match get("sort").as_str() {
            "" => SortKey::from_card_order(&self.settings.get().card_order),
            k => {
                let key = SortKey::parse(k).ok_or_else(|| {
                    bad(format!("sort must be one of {}", SortKey::ALL.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ")))
                })?;
                let descending = match get("dir").as_str() {
                    "" | "asc" => false,
                    "desc" => true,
                    _ => return Err(bad("dir must be asc or desc")),
                };
                (key, descending)
            }
        };
        Ok((sort_cards(g.store.cards_where(q.matcher()), key, descending, g.game.rarities()), key, descending))
    }

    pub(crate) fn find(&self, g: &GameService, key: &str) -> Result<Card> {
        g.store.get(key).ok_or_else(|| not_found("no such card"))
    }

    pub(crate) fn card(&self, g: &GameService, key: &str) -> Result {
        let c = self.find(g, key)?;
        let mut others = g.store.cards_where(|o| o.name == c.name && o.id != c.id);
        others.sort_by(|a, b| b.released_at.cmp(&a.released_at));
        let (label, url) = g.game.card_link(&c).unwrap_or_default();
        Ok(Reply::Json(json!({
            "card": view(&c), "raw": c, "details": g.game.details(&c), "link": { "label": label, "url": url }, "others": views(&others),
        })))
    }

    /// Every printing of the card in a language (the card's own when `lang` is "").
    pub(crate) fn printings(&self, g: &GameService, key: &str, lang: &str) -> Result {
        let c = self.find(g, key)?;
        let lang = if lang.is_empty() { c.language.as_str() } else { lang };
        let found = g.game.printings(&c, lang).map_err(upstream)?;
        Ok(Reply::Json(json!({ "current": c.id, "printings": views(&found) })))
    }

    pub(crate) fn update_card(&self, g: &GameService, key: &str, body: &[u8]) -> Result {
        let mut c = self.find(g, key)?;
        #[derive(Deserialize)]
        struct Req {
            /// Every purchase of the card: replaces its copies.
            #[serde(default)]
            purchases: Vec<PurchaseForm>,
        }
        let req: Req = decode(body)?;
        let before = lots_of(&c);
        c.copies = Default::default();
        for p in &req.purchases {
            p.validate(&c)?;
            p.add(&mut c);
        }
        let copies = c.copy_count();
        let (new_key, merged) = g.store.save_card(key, c.clone()).map_err(internal)?;
        // The lots the card's copies were in or are in now: their split changes.
        reprice_lots(g, before.into_iter().chain(lots_of(&c)))?;
        tracing::info!(game = g.game.id(), card = c.name, key = new_key, copies, "card updated");
        Ok(Reply::Json(json!({ "key": new_key, "merged": merged, "deleted": copies == 0 })))
    }

    pub(crate) fn delete_card(&self, g: &GameService, key: &str) -> Result {
        let c = self.find(g, key)?;
        g.store.remove_card(key).map_err(internal)?;
        reprice_lots(g, lots_of(&c))?;
        tracing::info!(game = g.game.id(), card = c.name, key, "card deleted");
        Ok(Reply::Json(json!({ "deleted": key })))
    }

    /// Adds a catalog card (as returned by search) with one purchase.
    pub(crate) fn add_card(&self, g: &GameService, body: &[u8]) -> Result {
        #[derive(Deserialize)]
        struct Req {
            #[serde(default)]
            card: Card,
            #[serde(flatten)]
            purchase: PurchaseForm,
        }
        let mut req: Req = decode(body)?;
        if req.card.id.is_empty() || req.purchase.quantity <= 0 {
            return Err(bad("a card and a quantity of at least 1 are required"));
        }
        req.purchase.validate(&req.card)?;
        // Added under a lot: the lot sets the price and date.
        let lot = match req.purchase.lot.as_str() {
            "" => None,
            id => Some(self.open_lot(g, id)?),
        };
        if let Some(lot) = &lot {
            req.purchase.price_paid = 0.0;
            req.purchase.acquired_date = lot.acquired_date.clone();
        }
        let mut c = req.card;
        c.copies = Default::default(); // only the purchase sent is added
        req.purchase.add(&mut c);
        let key = card_key(&c);
        let (new_cards, already_owned) = g.store.add_cards(vec![c.clone()]).map_err(internal)?;
        tracing::info!(game = g.game.id(), card = c.name, copies = c.copy_count(), already_owned = already_owned > 0, "card added");
        let mut answer = json!({ "key": key, "newCards": new_cards, "alreadyOwned": already_owned });
        if let Some(lot) = lot {
            g.reprice_lot(&lot).map_err(internal)?;
            answer["lot"] = lots::summary(g, &lot);
        }
        Ok(Reply::Json(answer))
    }

    /// Moves a card to another printing of the same card. The printing is looked up
    /// again rather than trusted from the request.
    pub(crate) fn change_printing(&self, g: &GameService, key: &str, body: &[u8]) -> Result {
        let c = self.find(g, key)?;
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Req {
            id: String,
            /// The language the printing was chosen in ("" for the card's own).
            lang: String,
        }
        let req: Req = decode(body).map_err(|_| bad("the printing's id is required"))?;
        if req.id.is_empty() {
            return Err(bad("the printing's id is required"));
        }
        let lang = if req.lang.is_empty() { c.language.clone() } else { req.lang };
        let found = g.game.printings(&c, &lang).map_err(upstream)?;
        let p = found.iter().find(|p| p.id == req.id).ok_or_else(|| bad("that isn't a printing of this card"))?;
        let moved = with_printing(&c, p);
        let finishes = moved.owned_finishes(g.game.finishes());
        let finish_changed = finishes != c.owned_finishes(g.game.finishes());
        let (new_key, merged) = g.store.save_card(key, moved.clone()).map_err(internal)?;
        tracing::info!(
            game = g.game.id(),
            card = c.name,
            from = format!("{} {}", c.set, c.collector_number),
            to = format!("{} {}", moved.set, moved.collector_number),
            "printing changed"
        );
        Ok(Reply::Json(json!({ "key": new_key, "merged": merged, "finishes": finishes, "finishChanged": finish_changed })))
    }

    pub(crate) fn bulk_edit(&self, g: &GameService, body: &[u8]) -> Result {
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Req {
            keys: Vec<String>,
            condition: String,
            finish: String,
            price_paid: Option<f64>,
            acquired_date: String,
            add_tags: Vec<String>,
            remove_tags: Vec<String>,
            clear_tags: bool,
            delete: bool,
        }
        let req: Req = decode(body).map_err(|_| bad("choose at least one card"))?;
        if req.keys.is_empty() {
            return Err(bad("choose at least one card"));
        }
        if req.price_paid.is_some_and(|p| p < 0.0) {
            return Err(bad("price paid can't be negative"));
        }
        check_date(&req.acquired_date)?;
        for t in req.remove_tags.iter().filter(|t| is_system_tag(t.trim())) {
            clean_user_tag(t).map_err(|e| bad(e.to_string()))?;
        }
        let change = BulkChange {
            condition: (!req.condition.is_empty()).then(|| Condition::parse(&req.condition)),
            finish: (!req.finish.is_empty()).then_some(req.finish),
            price_paid: req.price_paid,
            acquired_date: (!req.acquired_date.is_empty()).then_some(req.acquired_date),
            add_tags: tags_from(&req.add_tags)?,
            remove_tags: req.remove_tags,
            clear_tags: req.clear_tags,
            delete: req.delete,
        };
        if change.is_empty() {
            return Err(bad("choose at least one change"));
        }
        // A card that would end up with too many tags stops the change before the backup.
        if !change.delete {
            for c in req.keys.iter().filter_map(|k| g.store.get(k)) {
                change.apply(&c).map_err(|e| bad(format!("{e:#}")))?;
            }
        }
        let backup = g.store.backup(if change.delete { "bulk-delete" } else { "bulk-edit" }).map_err(internal)?;
        let res = g.store.bulk_edit(&req.keys, &change).map_err(internal)?;
        tracing::info!(game = g.game.id(), cards = req.keys.len(), updated = res.updated, deleted = res.deleted, "bulk edit");
        let mut answer = serde_json::to_value(res).map_err(|e| internal(e.into()))?;
        answer["undo"] = undo(g, &backup);
        Ok(Reply::Json(answer))
    }
}
