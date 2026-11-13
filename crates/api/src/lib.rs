//! TCG Binder's JSON API and the browser UI, as one request handler. Every front end
//! uses it: the desktop and iPhone apps call it in-process through a custom URL scheme,
//! and `tcgbinder --web` serves it over HTTP. The UI (the `ui` folder) is embedded in
//! release builds and read from disk in debug builds.
//!
//! See docs/web-api.md for the endpoints.

mod views;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use http::{HeaderValue, Method, StatusCode, header};
use serde::Deserialize;
use serde_json::{Value, json};

use tcgbinder_core::collection::{
    self, BulkChange, Card, Condition, Query, card_key, compute_stats, is_any, round_cents, set_options, sort_cards, top_by_each,
    unique_cards, with_printing,
};
use tcgbinder_core::game::finish_names;
use tcgbinder_core::scan::Reading;
use tcgbinder_core::service::{self, GameService};
use tcgbinder_core::settings::{Settings, SettingsStore, choices};
use tcgbinder_core::{Purchase, dates};

use views::{date_or_empty, points, view, views};

/// A request with its whole body.
pub type Request = http::Request<Vec<u8>>;
/// A response with its whole body.
pub type Response = http::Response<Vec<u8>>;

/// Must be sent with every request that changes data. Browsers won't let another
/// website add a custom header to a cross-origin request without a CORS preflight,
/// which is never approved, so this blocks cross-site requests.
pub const CLIENT_HEADER: &str = "X-TCGBinder-Client";

/// The largest request body accepted (an import file, a photo).
pub const MAX_BODY: usize = 64 << 20;

/// Reads the text off a photo of a card (Apple's Vision framework, where available).
pub type ReadImage = fn(&[u8]) -> anyhow::Result<Reading>;

/// Which front end the API is serving, for the UI to adapt to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Client {
    /// The desktop app.
    Desktop,
    /// The phone app.
    Phone,
    /// A web browser, through `--web`.
    Web,
}

impl Client {
    fn as_str(self) -> &'static str {
        match self {
            Client::Desktop => "desktop",
            Client::Phone => "phone",
            Client::Web => "web",
        }
    }
}

/// The copies owned of each catalog card.
fn owned(g: &GameService, cards: &[Card]) -> Vec<i64> {
    cards.iter().map(|c| g.store.get(&card_key(c)).map_or(0, |o| o.copy_count())).collect()
}

/// The owned cards of a set.
fn set_cards(g: &GameService, code: &str) -> Vec<Card> {
    g.store.cards_where(|c| c.set.eq_ignore_ascii_case(code))
}

fn check_date(date: &str) -> Result<()> {
    if date.is_empty() || dates::parse(date).is_some() { Ok(()) } else { Err(bad("acquired date must look like 2024-03-15")) }
}

/// The API for a set of games.
pub struct Api {
    name: String,
    version: String,
    client: Client,
    settings: Arc<SettingsStore>,
    games: Vec<Arc<GameService>>,
    /// "<game>/<job>": long jobs running.
    busy: Mutex<HashSet<String>>,
    read_image: Option<ReadImage>,
    /// Where exports are saved, for apps that can't download files.
    export_dir: Option<PathBuf>,
}

/// An error answer.
struct Error {
    status: StatusCode,
    message: String,
}

fn bad(msg: impl Into<String>) -> Error {
    Error { status: StatusCode::BAD_REQUEST, message: msg.into() }
}

fn not_found(msg: impl Into<String>) -> Error {
    Error { status: StatusCode::NOT_FOUND, message: msg.into() }
}

/// A failure talking to the game's catalog.
fn upstream(e: anyhow::Error) -> Error {
    Error { status: StatusCode::BAD_GATEWAY, message: format!("{e:#}") }
}

fn internal(e: anyhow::Error) -> Error {
    Error { status: StatusCode::INTERNAL_SERVER_ERROR, message: format!("{e:#}") }
}

/// What a handler answers.
enum Reply {
    Json(Value),
    /// A file download.
    File {
        name: String,
        content_type: &'static str,
        data: Vec<u8>,
    },
}

type Result<T = Reply> = std::result::Result<T, Error>;

/// The UI's files.
#[derive(rust_embed::RustEmbed)]
#[folder = "../../ui/"]
struct Ui;

/// The request's query parameters (the first value of each).
fn query_params(req: &Request) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for (k, v) in url::form_urlencoded::parse(req.uri().query().unwrap_or("").as_bytes()) {
        out.entry(k.into_owned()).or_insert_with(|| v.into_owned());
    }
    out
}

fn decode<T: for<'de> Deserialize<'de>>(body: &[u8]) -> Result<T> {
    serde_json::from_slice(body).map_err(|e| bad(e.to_string()))
}

impl Api {
    /// The API for `games` (already opened, each with its own files) with the user's
    /// settings.
    pub fn new(name: &str, version: &str, client: Client, settings: Arc<SettingsStore>, games: Vec<Arc<GameService>>) -> Api {
        Api {
            name: name.into(),
            version: version.into(),
            client,
            settings,
            games,
            busy: Mutex::default(),
            read_image: None,
            export_dir: None,
        }
    }

    /// Turns on card scanning with a photo reader.
    pub fn with_reader(mut self, read: ReadImage) -> Api {
        self.read_image = Some(read);
        self
    }

    /// Saves exports to `dir` (POST .../export) for apps that can't download files.
    pub fn with_export_dir(mut self, dir: PathBuf) -> Api {
        self.export_dir = Some(dir);
        self
    }

    /// Answers one request: the API under /api/, the UI's files otherwise.
    pub fn handle(&self, req: &Request) -> Response {
        let start = Instant::now();
        let path = req.uri().path();
        let changes = req.method() != Method::GET && req.method() != Method::HEAD;
        let result = if changes && !req.headers().contains_key(CLIENT_HEADER) {
            Err(Error { status: StatusCode::FORBIDDEN, message: format!("missing {CLIENT_HEADER} header") })
        } else if req.body().len() > MAX_BODY {
            Err(Error { status: StatusCode::PAYLOAD_TOO_LARGE, message: "the request is too large".into() })
        } else if let Some(rest) = path.strip_prefix("/api/") {
            self.route(req, rest)
        } else {
            return with_safety_headers(static_file(path));
        };
        let resp = match result {
            Ok(Reply::Json(v)) => json_response(StatusCode::OK, &v),
            Ok(Reply::File { name, content_type, data }) => http::Response::builder()
                .header(header::CONTENT_TYPE, content_type)
                .header(header::CONTENT_DISPOSITION, format!("attachment; filename=\"{name}\""))
                .header(header::CACHE_CONTROL, "no-store")
                .body(data)
                .unwrap(),
            Err(e) => json_response(e.status, &json!({ "error": e.message })),
        };
        tracing::debug!(method = %req.method(), path, status = resp.status().as_u16(), ms = start.elapsed().as_millis() as u64, "api");
        with_safety_headers(resp)
    }

    fn route(&self, req: &Request, path: &str) -> Result {
        let q = query_params(req);
        let m = req.method();
        let body = req.body();
        let parts: Vec<&str> = path.trim_end_matches('/').split('/').collect();
        match (m.as_str(), parts.as_slice()) {
            ("GET", ["info"]) => Ok(self.info()),
            ("GET", ["status"]) => Ok(self.status()),
            ("GET", ["search"]) => Ok(self.search_all(q.get("q").map_or("", String::as_str))),
            ("GET", ["settings"]) => Ok(self.get_settings()),
            ("PUT", ["settings"]) => self.put_settings(body),
            (_, ["games", id, rest @ ..]) => {
                let g = self.games.iter().find(|g| g.game.id() == *id).ok_or_else(|| not_found(format!("unknown game {id:?}")))?;
                let key = q.get("key").map_or("", String::as_str);
                match (m.as_str(), rest) {
                    ("GET", ["home"]) => Ok(self.home(g, q.get("range").map_or("", String::as_str))),
                    ("GET", ["filters"]) => Ok(self.filters(g)),
                    ("GET", ["cards"]) => Ok(self.cards(g, &q)),
                    ("POST", ["cards"]) => self.add_card(g, body),
                    ("GET", ["card"]) => self.card(g, key),
                    ("PUT", ["card"]) => self.update_card(g, key, body),
                    ("DELETE", ["card"]) => self.delete_card(g, key),
                    ("GET", ["printings"]) => self.printings(g, key),
                    ("PUT", ["card", "printing"]) => self.change_printing(g, key, body),
                    ("POST", ["bulk"]) => self.bulk_edit(g, body),
                    ("GET", ["set"]) => self.set_page(g, q.get("code").map_or("", String::as_str)),
                    ("GET", ["missing"]) => self.missing(g, q.get("set").map_or("", String::as_str)),
                    ("GET", ["search"]) => self.search(g, &q),
                    ("POST", ["import"]) => self.import(g, req),
                    ("POST", ["refresh"]) => self.job(g, "refresh", || {
                        let n = g.refresh_prices(&mut |_, _| {})?;
                        Ok(json!({ "updated": n }))
                    }),
                    ("POST", ["price-history"]) => self.job(g, "price-history", || {
                        let n = g.load_price_history(&mut |_| {})?;
                        Ok(json!({ "printings": n }))
                    }),
                    ("GET", ["export"]) => self.export(g),
                    ("POST", ["export"]) => self.save_export(g),
                    ("POST", ["scan"]) => self.scan(g, body),
                    ("POST", ["scan", "frame"]) => self.scan_frame(g, body),
                    _ => Err(not_found(format!("no {m} /api/{path}"))),
                }
            }
            _ => Err(not_found(format!("no {m} /api/{path}"))),
        }
    }

    /// Runs a long operation unless one of the same kind is already running.
    fn job(&self, g: &GameService, name: &str, run: impl FnOnce() -> anyhow::Result<Value>) -> Result {
        let key = format!("{}/{name}", g.game.id());
        if !self.busy.lock().unwrap().insert(key.clone()) {
            return Err(Error { status: StatusCode::CONFLICT, message: "already running".into() });
        }
        let res = run();
        self.busy.lock().unwrap().remove(&key);
        res.map(Reply::Json).map_err(upstream)
    }

    fn scanning(&self, g: &GameService) -> bool {
        self.read_image.is_some() && g.game.scanner().is_some()
    }

    // --- Read endpoints ----------------------------------------------------------

    fn info(&self) -> Reply {
        let games: Vec<Value> =
            self.games.iter().map(|g| json!({ "ID": g.game.id(), "Name": g.game.name(), "Scan": self.scanning(g) })).collect();
        Reply::Json(json!({
            "name": self.name, "version": self.version, "games": games, "ranges": choices::CHART_RANGES,
            "client": self.client.as_str(), "saveExports": self.export_dir.is_some(),
        }))
    }

    /// A number that changes whenever any collection does, so open pages can refresh
    /// after background work (such as the daily price refresh).
    fn status(&self) -> Reply {
        let revision: u64 = self.games.iter().map(|g| g.store.revision()).sum();
        Reply::Json(json!({ "revision": revision }))
    }

    fn home(&self, g: &GameService, range: &str) -> Reply {
        let cards = g.store.cards();
        let st = compute_stats(&cards, &g.cached_sets());
        let range = if range.is_empty() { service::DEFAULT_CHART_RANGE } else { range };
        let vc = service::build_value_chart(&cards, &st, &g.prices, range, dates::today());
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

    fn filters(&self, g: &GameService) -> Reply {
        let sets: Vec<Value> = set_options(&g.store.cards()).into_iter().map(|o| json!({ "Code": o.code, "Label": o.label })).collect();
        let extra: Vec<Value> = g.game.filters().iter().map(|f| json!({ "Name": f.name, "Options": f.options })).collect();
        let (text, set) = g.game.search_hints();
        Reply::Json(json!({
            "sets": sets, "rarities": g.game.rarities(), "finishes": g.game.finishes(), "finishNames": finish_names(g.game.as_ref()),
            "filters": extra, "conditions": choices::CONDITIONS, "searchHints": [text, set],
        }))
    }

    /// The filtered cards in the Card order setting's order, with counts. Parameters: q,
    /// set, rarity, finish, and one per game filter, named f.<filter name>.
    fn cards(&self, g: &GameService, params: &HashMap<String, String>) -> Reply {
        let filters = g.game.filters();
        let get = |k: &str| params.get(k).cloned().unwrap_or_default();
        let q = Query {
            text: get("q"),
            set: get("set"),
            rarity: get("rarity"),
            finish: get("finish"),
            matches: Some(Box::new(|c: &Card| {
                filters.iter().all(|f| params.get(&format!("f.{}", f.name)).is_none_or(|opt| is_any(opt) || (f.matches)(c, opt)))
            })),
        };
        let filtered = sort_cards(g.store.cards_where(q.matcher()), &self.settings.get().card_order);
        let card_count: i64 = filtered.iter().map(Card::copy_count).sum();
        let sets: HashSet<String> = filtered.iter().map(|c| c.set.to_uppercase()).collect();
        Reply::Json(json!({
            "total": g.store.len(), "matched": filtered.len(), "cardCount": card_count,
            "uniqueCards": unique_cards(&filtered), "sets": sets.len(), "cards": views(&filtered),
        }))
    }

    fn find(&self, g: &GameService, key: &str) -> Result<Card> {
        g.store.get(key).ok_or_else(|| not_found("no such card"))
    }

    fn card(&self, g: &GameService, key: &str) -> Result {
        let c = self.find(g, key)?;
        let mut others = g.store.cards_where(|o| o.name == c.name && o.id != c.id);
        others.sort_by(|a, b| b.released_at.cmp(&a.released_at));
        let (label, url) = g.game.card_link(&c).unwrap_or_default();
        Ok(Reply::Json(json!({
            "card": view(&c), "raw": c, "details": g.game.details(&c), "link": { "label": label, "url": url }, "others": views(&others),
        })))
    }

    fn search(&self, g: &GameService, params: &HashMap<String, String>) -> Result {
        let get = |k: &str| params.get(k).map_or("", String::as_str);
        let page = g.game.search(get("text"), get("set"), get("next")).map_err(upstream)?;
        // Cards already owned say so.
        Ok(Reply::Json(json!({ "cards": views(&page.cards), "owned": owned(g, &page.cards), "total": page.total, "next": page.next_page })))
    }

    fn set_page(&self, g: &GameService, code: &str) -> Result {
        let code = code.to_uppercase();
        let cards = set_cards(g, &code);
        let st = compute_stats(&cards, &g.cached_sets());
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

    fn missing(&self, g: &GameService, code: &str) -> Result {
        let code = code.to_uppercase();
        let set_cards = g.game.set_cards(&code).map_err(upstream)?;
        let missing = collection::missing(&set_cards, &self::set_cards(g, &code));
        Ok(Reply::Json(json!({ "set": code, "total": set_cards.len(), "missing": views(&missing) })))
    }

    fn printings(&self, g: &GameService, key: &str) -> Result {
        let c = self.find(g, key)?;
        let found = g.game.printings(&c).map_err(upstream)?;
        Ok(Reply::Json(json!({ "current": c.id, "printings": views(&found) })))
    }

    /// Searches every game's collection.
    fn search_all(&self, q: &str) -> Reply {
        let q = q.trim();
        let games: Vec<Value> = self
            .games
            .iter()
            .map(|g| {
                let matches = sort_cards(g.store.cards_where(Query { text: q.into(), ..Default::default() }.matcher()), "name");
                let shown = &matches[..matches.len().min(200)];
                json!({ "Game": g.game.id(), "Name": g.game.name(), "Total": matches.len(), "Cards": views(shown) })
            })
            .collect();
        Reply::Json(json!({ "q": q, "games": games }))
    }

    // --- Write endpoints ---------------------------------------------------------

    fn update_card(&self, g: &GameService, key: &str, body: &[u8]) -> Result {
        let mut c = self.find(g, key)?;
        #[derive(Deserialize)]
        struct Req {
            /// Every purchase of the card: replaces its copies.
            #[serde(default)]
            purchases: Vec<PurchaseForm>,
        }
        let req: Req = decode(body)?;
        c.copies = Default::default();
        for p in &req.purchases {
            p.validate(&c)?;
            p.add(&mut c);
        }
        let copies = c.copy_count();
        let (new_key, merged) = g.store.save_card(key, c.clone()).map_err(internal)?;
        tracing::info!(game = g.game.id(), card = c.name, key = new_key, copies, "card updated");
        Ok(Reply::Json(json!({ "key": new_key, "merged": merged, "deleted": copies == 0 })))
    }

    fn delete_card(&self, g: &GameService, key: &str) -> Result {
        let c = self.find(g, key)?;
        g.store.remove_card(key).map_err(internal)?;
        tracing::info!(game = g.game.id(), card = c.name, key, "card deleted");
        Ok(Reply::Json(json!({ "deleted": key })))
    }

    /// Adds a catalog card (as returned by search) with one purchase.
    fn add_card(&self, g: &GameService, body: &[u8]) -> Result {
        #[derive(Deserialize)]
        struct Req {
            #[serde(default)]
            card: Card,
            #[serde(flatten)]
            purchase: PurchaseForm,
        }
        let req: Req = decode(body)?;
        if req.card.id.is_empty() || req.purchase.quantity <= 0 {
            return Err(bad("a card and a quantity of at least 1 are required"));
        }
        req.purchase.validate(&req.card)?;
        let mut c = req.card;
        c.copies = Default::default(); // only the purchase sent is added
        req.purchase.add(&mut c);
        let key = card_key(&c);
        let (new_cards, already_owned) = g.store.add_cards(vec![c.clone()]).map_err(internal)?;
        tracing::info!(game = g.game.id(), card = c.name, copies = c.copy_count(), already_owned = already_owned > 0, "card added");
        Ok(Reply::Json(json!({ "key": key, "newCards": new_cards, "alreadyOwned": already_owned })))
    }

    /// Moves a card to another printing of the same card. The printing is looked up
    /// again rather than trusted from the request.
    fn change_printing(&self, g: &GameService, key: &str, body: &[u8]) -> Result {
        let c = self.find(g, key)?;
        #[derive(Deserialize)]
        struct Req {
            #[serde(default)]
            id: String,
        }
        let req: Req = decode(body).map_err(|_| bad("the printing's id is required"))?;
        if req.id.is_empty() {
            return Err(bad("the printing's id is required"));
        }
        let found = g.game.printings(&c).map_err(upstream)?;
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

    fn bulk_edit(&self, g: &GameService, body: &[u8]) -> Result {
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Req {
            keys: Vec<String>,
            condition: String,
            finish: String,
            price_paid: Option<f64>,
            acquired_date: String,
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
        let change = BulkChange {
            condition: (!req.condition.is_empty()).then(|| Condition::parse(&req.condition)),
            finish: (!req.finish.is_empty()).then_some(req.finish),
            price_paid: req.price_paid,
            acquired_date: (!req.acquired_date.is_empty()).then_some(req.acquired_date),
            delete: req.delete,
        };
        if change.is_empty() {
            return Err(bad("choose at least one change"));
        }
        let res = g.store.bulk_edit(&req.keys, &change).map_err(internal)?;
        tracing::info!(game = g.game.id(), cards = req.keys.len(), updated = res.updated, deleted = res.deleted, "bulk edit");
        Ok(Reply::Json(serde_json::to_value(res).map_err(|e| internal(e.into()))?))
    }

    /// Imports the file sent as a multipart "file" field or as the raw body.
    fn import(&self, g: &GameService, req: &Request) -> Result {
        let content_type = req.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("");
        let data = if content_type.starts_with("multipart/") {
            multipart_file(content_type, req.body()).ok_or_else(|| bad("no \"file\" in the upload"))?
        } else {
            req.body().clone()
        };
        self.job(g, "import", || {
            let res = g.import(&data, &mut |_, _| {})?;
            Ok(json!({
                "summary": res.summary(), "cards": res.cards, "newCards": res.new_cards,
                "alreadyOwned": res.already_owned, "format": res.report.format,
            }))
        })
    }

    fn export_name(g: &GameService) -> String {
        format!("{}-collection-{}.json", g.game.id(), dates::today_string())
    }

    /// Downloads the game's collection file, which import reads back.
    fn export(&self, g: &GameService) -> Result {
        let data = std::fs::read(g.store.file_path()).map_err(|e| internal(e.into()))?;
        Ok(Reply::File { name: Self::export_name(g), content_type: "application/json", data })
    }

    /// Saves a copy of the collection file to the export folder.
    fn save_export(&self, g: &GameService) -> Result {
        let dir = self.export_dir.as_ref().ok_or_else(|| not_found("this app downloads exports instead (GET)"))?;
        let path = dir.join(Self::export_name(g));
        std::fs::create_dir_all(dir).and_then(|_| std::fs::copy(g.store.file_path(), &path)).map_err(|e| internal(e.into()))?;
        tracing::info!(game = g.game.id(), path = %path.display(), "collection exported");
        Ok(Reply::Json(json!({ "path": path.display().to_string() })))
    }

    fn read_photo(&self, g: &GameService, body: &[u8]) -> Result<Reading> {
        let read = self.read_image.filter(|_| self.scanning(g)).ok_or_else(|| Error {
            status: StatusCode::NOT_IMPLEMENTED,
            message: "card scanning isn't available here (it needs macOS or iOS)".into(),
        })?;
        read(body).map_err(|e| bad(format!("{e:#}")))
    }

    /// Reads a photo of a card and looks it up: the printings it could be, best first.
    fn scan(&self, g: &GameService, body: &[u8]) -> Result {
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
    fn scan_frame(&self, g: &GameService, body: &[u8]) -> Result {
        let reading = self.read_photo(g, body)?;
        let (key, complete) = g.game.scanner().expect("checked by read_photo").scan_key(&reading);
        Ok(Reply::Json(json!({ "key": key, "complete": complete, "lines": reading.lines.len() })))
    }

    // --- Settings ----------------------------------------------------------------

    fn get_settings(&self) -> Reply {
        let data_dir = self.settings.path().and_then(|p| p.parent()).map(|d| d.display().to_string()).unwrap_or_default();
        let choices: serde_json::Map<String, Value> = choices::ALL.iter().map(|(k, v)| (k.to_string(), json!(v))).collect();
        Reply::Json(json!({ "settings": self.settings.get(), "choices": choices, "defaults": Settings::default(), "dataDir": data_dir }))
    }

    /// Changes the settings sent; settings left out keep their values.
    fn put_settings(&self, body: &[u8]) -> Result {
        let change: Value = decode(body)?;
        let next = self.settings.get().with_changes(&change).map_err(|e| bad(e.to_string()))?;
        self.settings.set(next).map_err(|e| bad(e.to_string()))?;
        Ok(Reply::Json(json!({ "settings": self.settings.get() })))
    }
}

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
        check_date(&self.acquired_date)
    }

    /// Adds the purchase to `c` (in the printing's first finish if none is given).
    fn add(&self, c: &mut Card) {
        let finish = if self.finish.is_empty() { c.finishes.first().cloned().unwrap_or_default() } else { self.finish.clone() };
        let p = Purchase { quantity: self.quantity, price_paid: self.price_paid, acquired_date: self.acquired_date.clone() };
        c.add_purchase(&finish, Condition::parse(&self.condition), p);
    }
}

/// The contents of a multipart upload's "file" field.
fn multipart_file(content_type: &str, body: &[u8]) -> Option<Vec<u8>> {
    let boundary = content_type.split(';').find_map(|p| p.trim().strip_prefix("boundary="))?.trim_matches('"');
    let delim = format!("--{boundary}").into_bytes();
    let find = |hay: &[u8], needle: &[u8], from: usize| hay[from..].windows(needle.len()).position(|w| w == needle).map(|i| i + from);
    let mut at = find(body, &delim, 0)?;
    loop {
        let start = at + delim.len();
        let next = find(body, &delim, start)?;
        let part = &body[start..next];
        let header_end = find(part, b"\r\n\r\n", 0)?;
        let headers = String::from_utf8_lossy(&part[..header_end]).to_lowercase();
        if headers.contains("name=\"file\"") {
            let content = &part[header_end + 4..];
            return Some(content.strip_suffix(b"\r\n").unwrap_or(content).to_vec());
        }
        at = next;
    }
}

fn json_response(status: StatusCode, v: &Value) -> Response {
    http::Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json; charset=utf-8")
        .header(header::CACHE_CONTROL, "no-store")
        .body(serde_json::to_vec(v).unwrap_or_default())
        .unwrap()
}

fn with_safety_headers(mut resp: Response) -> Response {
    let h = resp.headers_mut();
    h.insert("X-Content-Type-Options", HeaderValue::from_static("nosniff"));
    h.insert("Referrer-Policy", HeaderValue::from_static("no-referrer"));
    resp
}

/// One of the UI's files.
fn static_file(path: &str) -> Response {
    let name = match path.trim_start_matches('/') {
        "" => "index.html",
        p => p,
    };
    match Ui::get(name) {
        Some(f) => http::Response::builder()
            .header(header::CONTENT_TYPE, mime_guess::from_path(name).first_or_octet_stream().as_ref())
            .header(header::CACHE_CONTROL, "no-cache")
            .body(f.data.into_owned())
            .unwrap(),
        None => http::Response::builder().status(StatusCode::NOT_FOUND).body(b"not found".to_vec()).unwrap(),
    }
}

/// Whether a Host header names this computer: localhost or an IP address. Servers on a
/// loopback address answer only such requests, which defeats DNS rebinding.
pub fn is_local_host(host: &str) -> bool {
    let name = match host.rsplit_once(':') {
        Some((h, port)) if port.bytes().all(|b| b.is_ascii_digit()) && !h.ends_with(':') => h,
        _ => host,
    };
    let name = name.trim_start_matches('[').trim_end_matches(']');
    name.eq_ignore_ascii_case("localhost") || name.parse::<std::net::IpAddr>().is_ok()
}

/// A JSON error answer, for front ends that refuse a request before the API sees it.
pub fn error_response(status: StatusCode, message: &str) -> Response {
    with_safety_headers(json_response(status, &json!({ "error": message })))
}

#[cfg(test)]
mod tests;
