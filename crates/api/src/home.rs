//! The read-only pages: app info, the change revision, Home, the filter choices,
//! Statistics, a set's page and its missing cards, and the search across games.

use crate::views::{date_or_empty, points, view, views};
use crate::{Api, Reply, Result, not_found, sales, sealed, upstream, watchlist};
use serde_json::{Value, json};
use tcgbinder_core::collection::{
    self, Card, Query, SortKey, compute_stats, round_cents, set_options, sort_cards, statistics, tag_summaries, top_by_each,
};
use tcgbinder_core::dates;
use tcgbinder_core::game::finish_names;
use tcgbinder_core::service::GameService;
use tcgbinder_core::settings::choices;

/// The owned cards of a set.
fn set_cards(g: &GameService, code: &str) -> Vec<Card> {
    g.store.cards_where(|c| c.set.eq_ignore_ascii_case(code))
}

impl Api {
    pub(crate) fn info(&self) -> Reply {
        let games: Vec<Value> =
            self.games.iter().map(|g| json!({ "ID": g.game.id(), "Name": g.game.name(), "Scan": self.scanning(g) })).collect();
        Reply::Json(json!({
            "name": self.name, "version": self.version, "games": games, "ranges": choices::CHART_RANGES,
            "client": self.client.as_str(), "saveExports": self.export_dir.is_some(), "opensLinks": self.open_url.is_some(),
        }))
    }

    /// A number that changes whenever any collection does, so open pages can refresh
    /// after background work (such as the daily price refresh) or after another device
    /// changed the files: each game's files are checked for changes first.
    pub(crate) fn status(&self) -> Reply {
        for g in &self.games {
            g.refresh_files();
        }
        let revision: u64 = self.games.iter().map(|g| g.revision()).sum();
        Reply::Json(json!({ "revision": revision }))
    }

    pub(crate) fn home(&self, g: &GameService, range: &str) -> Reply {
        let cards = g.store.cards();
        let st = compute_stats(&cards, &g.cached_sets(), self.main_set_only());
        let range = if range.is_empty() { collection::DEFAULT_CHART_RANGE } else { range };
        let vc = collection::build_value_chart(&cards, &st, &g.prices, range, dates::today());
        let sets: Vec<Value> = st
            .by_set
            .iter()
            .map(|s| {
                json!({
                    "Code": s.code, "Name": s.name, "Released": s.released, "Collected": s.collected, "Size": s.set_size,
                    "Cards": s.cards, "UniqueCards": s.unique_cards, "Completion": s.completion().unwrap_or(-1.0),
                    "PricePaid": round_cents(s.price_paid), "MarketValue": round_cents(s.market_value),
                })
            })
            .collect();
        let top: Vec<_> = st.top_cards.iter().map(|cv| view(&cv.card)).collect();
        let src = g.game.price_history();
        Reply::Json(json!({
            "game": g.game.id(), "name": g.game.name(),
            "cards": st.cards, "uniqueCards": st.unique_cards, "sets": st.sets,
            "pricePaid": round_cents(st.price_paid), "pricePaidUnknown": st.price_paid_unknown,
            "marketValue": round_cents(st.market_value), "unpriced": st.unpriced, "pricesAsOf": st.prices_as_of,
            "realisedThisYear": round_cents(sales::realised_this_year(g)), "hasSales": !g.sales.all().is_empty(),
            "alerts": watchlist::alerts(g), "sealed": sealed::home_summary(g),
            "bySet": sets, "topCards": top,
            "chart": {
                "range": range, "from": date_or_empty(vc.from), "to": dates::format(vc.to),
                "historyStart": date_or_empty(vc.history_start),
                "marketValue": points(&vc.market_value), "pricePaid": points(&vc.price_paid), "estimate": points(&vc.estimate),
            },
            "canLoadHistory": src.is_some(), "historyLoaded": g.prices.history_loaded_on(),
            "historyInfo": src.map_or("", |s| s.info()),
        }))
    }

    /// The Statistics page: breakdowns (each with the collection list's filter
    /// parameter for its rows), value bands, copies and money by month acquired, the
    /// cards with the most copies, and each tag's totals over its tagged copies.
    pub(crate) fn statistics(&self, g: &GameService) -> Reply {
        let cards = g.store.cards();
        let s = statistics(&cards, &g.game.breakdowns(), g.game.rarities(), g.game.finishes());
        let mut most: Vec<&Card> = cards.iter().filter(|c| c.copy_count() > 1).collect();
        most.sort_by(|a, b| b.copy_count().cmp(&a.copy_count()).then_with(|| a.name.cmp(&b.name)));
        let most: Vec<Card> = most.into_iter().take(10).cloned().collect();
        Reply::Json(json!({
            "breakdowns": s.breakdowns, "bands": s.bands, "byMonth": s.by_month, "undated": s.undated, "mostCopies": views(&most),
            "tags": tag_summaries(&cards),
        }))
    }

    pub(crate) fn filters(&self, g: &GameService) -> Reply {
        let sets: Vec<Value> = set_options(&g.store.cards()).into_iter().map(|o| json!({ "Code": o.code, "Label": o.label })).collect();
        let extra: Vec<Value> = g.game.filters().iter().map(|f| json!({ "Name": f.name, "Options": f.options })).collect();
        let (text, set) = g.game.search_hints();
        Reply::Json(json!({
            "sets": sets, "rarities": g.game.rarities(), "finishes": g.game.finishes(), "finishNames": finish_names(g.game.as_ref()),
            "filters": extra, "conditions": choices::CONDITIONS, "searchHints": [text, set], "tags": g.store.tag_names(),
            "languages": g.game.languages().iter().map(|(code, name)| json!({ "code": code, "name": name })).collect::<Vec<_>>(),
        }))
    }

    pub(crate) fn set_page(&self, g: &GameService, code: &str) -> Result {
        let code = code.to_uppercase();
        let cards = set_cards(g, &code);
        let st = compute_stats(&cards, &g.cached_sets(), self.main_set_only());
        let set = st.by_set.first().ok_or_else(|| not_found(format!("no cards from set {code:?}")))?;
        let top = top_by_each(&cards, 10); // ranked by one copy
        Ok(Reply::Json(json!({
            "code": set.code, "name": set.name, "released": set.released,
            "collected": set.collected, "size": set.set_size, "completion": set.completion().unwrap_or(-1.0),
            "cards": set.cards, "uniqueCards": set.unique_cards,
            "pricePaid": round_cents(set.price_paid), "marketValue": round_cents(set.market_value),
            "top": top.iter().map(|cv| view(&cv.card)).collect::<Vec<_>>(),
        })))
    }

    /// Whether set completion counts only the printed set (the Set completion setting).
    pub(crate) fn main_set_only(&self) -> bool {
        self.settings.get().set_completion != "all"
    }

    /// The set's cards not owned, by collector number, following the Set completion
    /// setting, with `totalCost`: one of each at market price, in the usual finish
    /// where it has a price. `printedSize` is the printed run used, or null for all.
    pub(crate) fn missing(&self, g: &GameService, code: &str) -> Result {
        let code = code.to_uppercase();
        let set_cards = g.game.set_cards(&code).map_err(upstream)?;
        let printed = g.cached_sets().get(&code).map_or(0, |s| s.printed_size);
        let printed = (self.main_set_only() && printed > 0).then_some(printed);
        let missing = collection::missing(&set_cards, &self::set_cards(g, &code), printed);
        let order = g.game.finishes();
        let price = |c: &Card| order.iter().find_map(|f| c.market.get(*f).copied()).or_else(|| c.market.values().copied().reduce(f64::min));
        let cost: f64 = missing.iter().filter_map(price).sum();
        let unpriced = missing.iter().filter(|c| price(c).is_none()).count();
        let total = set_cards.iter().filter(|c| printed.is_none_or(|n| collection::in_printed_set(&c.collector_number, n))).count();
        Ok(Reply::Json(json!({
            "set": code, "total": total, "printedSize": printed, "totalCost": round_cents(cost), "unpriced": unpriced,
            "missing": views(&missing),
        })))
    }

    /// Searches every game's collection.
    pub(crate) fn search_all(&self, q: &str) -> Reply {
        let q = q.trim();
        let games: Vec<Value> = self
            .games
            .iter()
            .map(|g| {
                let matches =
                    sort_cards(g.store.cards_where(Query { text: q.into(), ..Default::default() }.matcher()), SortKey::Name, false, &[]);
                let shown = &matches[..matches.len().min(200)];
                json!({ "Game": g.game.id(), "Name": g.game.name(), "Total": matches.len(), "Cards": views(shown) })
            })
            .collect();
        Reply::Json(json!({ "q": q, "games": games }))
    }
}
