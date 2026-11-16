//! TCG Binder's JSON API and the browser UI, as one request handler. Every front end
//! uses it: the desktop and iPhone apps call it in-process through a custom URL scheme,
//! and `tcgbinder --web` serves it over HTTP. The UI (the `ui` folder) is embedded in
//! release builds and read from disk in debug builds.
//!
//! See docs/web-api.md for the endpoints.

mod folders;
mod lists;
mod lots;
mod report;
mod sales;
mod sealed;
mod views;
mod watchlist;

pub use folders::{Folders, PickFolder, Restart};

/// Opens a web page in the system's browser (the apps' web views can't open new
/// windows themselves).
pub type OpenUrl = Arc<dyn Fn(&str) -> anyhow::Result<()> + Send + Sync>;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use http::{HeaderValue, Method, StatusCode, header};
use serde::Deserialize;
use serde_json::{Value, json};

use tcgbinder_core::collection::{
    self, BulkChange, Card, Condition, Query, SortKey, card_key, compute_stats, is_any, round_cents, set_options, sort_cards, statistics,
    top_by_each, unique_cards, with_printing, write_csv,
};
use tcgbinder_core::collection::{TagEdit, clean_tag, normalize_tags, same_tag, split_tags, tag_summaries};
use tcgbinder_core::game::finish_names;
use tcgbinder_core::scan::Reading;
use tcgbinder_core::service::{self, GameService, ImportPreview, ImportResult};
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

/// Checks and normalizes tags sent by a client: a 400 with the reason if any is
/// invalid.
fn tags_from(raw: &[String]) -> Result<Vec<String>> {
    normalize_tags(raw).map_err(|e| bad(e.to_string()))
}

/// Adds `tags` to every purchase of `cards` (an import's "tag every card").
fn tag_every_purchase(cards: &mut [Card], tags: &[String]) -> anyhow::Result<()> {
    if tags.is_empty() {
        return Ok(());
    }
    for c in cards {
        for list in c.copies.0.values_mut().flat_map(|m| m.values_mut()) {
            for p in list {
                let mut all = std::mem::take(&mut p.tags);
                all.extend(tags.iter().cloned());
                p.tags = normalize_tags(&all).map_err(|e| e.context(format!("{} ({} · {})", c.name, c.set, c.collector_number)))?;
            }
        }
    }
    Ok(())
}

/// A tag rename's or delete's answer: `changed`, and `undo` for its backup.
fn tag_edit_answer(g: &GameService, edit: &TagEdit) -> Value {
    let mut answer = json!({ "changed": edit.changed });
    if let Some(b) = &edit.backup {
        answer["undo"] = undo(g, b);
    }
    answer
}

/// The API for a set of games.
pub struct Api {
    name: String,
    version: String,
    client: Client,
    settings: Arc<SettingsStore>,
    games: Vec<Arc<GameService>>,
    /// "<game>/<job>" → the progress of each long job running.
    busy: Mutex<HashMap<String, JobProgress>>,
    read_image: Option<ReadImage>,
    /// Where exports are saved, for apps that can't download files.
    export_dir: Option<PathBuf>,
    /// Game ID → the import preview waiting to be committed (at most one per game).
    previews: Mutex<HashMap<String, PendingImport>>,
    folders: Option<Folders>,
    pick_folder: Option<PickFolder>,
    restart: Option<Restart>,
    open_url: Option<OpenUrl>,
    /// The data folder was changed: until the app restarts, nothing may be saved.
    switched: AtomicBool,
}

/// How far a long job has got.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct JobProgress {
    /// Steps done and in all (both 0 when the job can't count them).
    pub done: usize,
    pub total: usize,
    /// What it's doing now, e.g. "Downloading price history".
    pub message: String,
}

/// Lets a job report its progress, for `GET .../jobs`.
pub struct Reporter<'a> {
    api: &'a Api,
    key: String,
}

impl Reporter<'_> {
    /// Records how far the job has got.
    pub fn report(&self, done: usize, total: usize, message: &str) {
        if let Some(p) = self.api.busy.lock().unwrap().get_mut(&self.key) {
            *p = JobProgress { done, total, message: message.to_string() };
        }
    }
}

/// How long an import preview can wait to be committed.
const PREVIEW_LIFETIME: Duration = Duration::from_secs(15 * 60);

/// An import preview kept until it is committed, replaced or too old.
struct PendingImport {
    token: String,
    made: Instant,
    preview: ImportPreview,
}

/// A token no other preview in this process has had.
fn new_token() -> String {
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    format!("{nanos:x}-{:x}", COUNT.fetch_add(1, Ordering::Relaxed))
}

/// What an import did, as the API answers it.
fn import_result(g: &GameService, res: &ImportResult) -> Value {
    json!({
        "summary": res.summary(), "cards": res.cards, "newCards": res.new_cards,
        "alreadyOwned": res.already_owned, "format": res.report.format, "undo": undo(g, &res.backup),
    })
}

/// How to undo a change: restore `backup`, as long as the collection is still at the
/// revision the change left it at.
fn undo(g: &GameService, backup: &str) -> Value {
    json!({ "backup": backup, "revision": g.store.revision() })
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
            previews: Mutex::default(),
            folders: None,
            pick_folder: None,
            restart: None,
            open_url: None,
            switched: AtomicBool::new(false),
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
        } else if changes && self.switched.load(Ordering::SeqCst) {
            Err(Error { status: StatusCode::CONFLICT, message: "the data folder has changed: restart TCG Binder to use it".into() })
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
            ("GET", ["data-folder"]) => self.data_folder(),
            ("POST", ["data-folder"]) => self.change_data_folder(body),
            ("POST", ["data-folder", "pick"]) => self.pick_data_folder(),
            ("POST", ["open"]) => self.open_link(body),
            ("POST", ["data-folder", "inspect"]) => self.inspect_data_folder(body),
            ("GET", ["search"]) => Ok(self.search_all(q.get("q").map_or("", String::as_str))),
            ("GET", ["settings"]) => Ok(self.get_settings()),
            ("PUT", ["settings"]) => self.put_settings(body),
            (_, ["games", id, rest @ ..]) => {
                let g = self.games.iter().find(|g| g.game.id() == *id).ok_or_else(|| not_found(format!("unknown game {id:?}")))?;
                let key = q.get("key").map_or("", String::as_str);
                match (m.as_str(), rest) {
                    ("GET", ["home"]) => Ok(self.home(g, q.get("range").map_or("", String::as_str))),
                    ("GET", ["filters"]) => Ok(self.filters(g)),
                    ("GET", ["stats"]) => Ok(self.statistics(g)),
                    ("GET", ["cards"]) => self.cards(g, &q),
                    ("POST", ["cards"]) => self.add_card(g, body),
                    ("GET", ["card"]) => self.card(g, key),
                    ("PUT", ["card"]) => self.update_card(g, key, body),
                    ("DELETE", ["card"]) => self.delete_card(g, key),
                    ("GET", ["printings"]) => self.printings(g, key, q.get("lang").map_or("", String::as_str)),
                    ("PUT", ["card", "printing"]) => self.change_printing(g, key, body),
                    ("POST", ["card", "sell"]) => self.sell(g, key, body),
                    ("GET", ["sales"]) => self.sales(g, &q),
                    ("GET", ["tags"]) => Ok(self.tags(g)),
                    ("PUT", ["tags"]) => self.rename_tag(g, q.get("name").map_or("", String::as_str), body),
                    ("DELETE", ["tags"]) => self.delete_tag(g, q.get("name").map_or("", String::as_str)),
                    ("GET", ["sealed"]) => Ok(self.sealed(g)),
                    ("POST", ["sealed"]) => self.add_sealed(g, body),
                    ("PUT", ["sealed"]) => self.update_sealed(g, &q, body),
                    ("DELETE", ["sealed"]) => self.delete_sealed(g, &q),
                    ("GET", ["sealed", "products"]) => self.sealed_products(g, &q),
                    ("GET", ["watchlist"]) => Ok(self.watchlist(g)),
                    ("POST", ["watchlist"]) => self.add_watch(g, body),
                    ("PUT", ["watchlist"]) => self.change_watch(g, &q, body),
                    ("DELETE", ["watchlist"]) => self.delete_watch(g, &q),
                    ("GET", ["lists"]) => match q.get("id") {
                        Some(_) => self.list(g, &q),
                        None => Ok(self.lists(g)),
                    },
                    ("POST", ["lists"]) => self.create_list(g, body),
                    ("PUT", ["lists"]) => self.update_list(g, &q, body),
                    ("DELETE", ["lists"]) => self.delete_list(g, &q),
                    ("POST", ["lists", "entries"]) => self.add_list_entry(g, &q, body),
                    ("GET", ["lists", "export"]) => self.export_list(g, &q),
                    ("POST", ["lists", "import"]) => self.import_list(g, &q, body),
                    ("DELETE", ["sales"]) => self.delete_sale(g, &q),
                    ("POST", ["bulk"]) => self.bulk_edit(g, body),
                    ("GET", ["lots"]) => match q.get("id") {
                        Some(id) => self.lot(g, id),
                        None => Ok(self.list_lots(g)),
                    },
                    ("POST", ["lots"]) => self.create_lot(g, body),
                    ("POST", ["lots", "cards"]) => self.put_cards_in_lot(g, q.get("id").map_or("", String::as_str), body),
                    ("DELETE", ["lots", "cards"]) => self.remove_card_from_lot(g, q.get("id").map_or("", String::as_str), key),
                    ("PUT", ["lots"]) => self.update_lot(g, q.get("id").map_or("", String::as_str), body),
                    ("DELETE", ["lots"]) => {
                        self.delete_lot(g, q.get("id").map_or("", String::as_str), q.get("cards").map_or("", String::as_str))
                    }
                    ("GET", ["backups"]) => Ok(Reply::Json(json!(g.store.backups().map_err(internal)?))),
                    ("POST", ["backups", "restore"]) => self.restore_backup(g, body),
                    ("POST", ["conflicts", "merge"]) => self.merge_conflict(g, body),
                    ("POST", ["conflicts", "discard"]) => self.discard_conflict(g, body),
                    ("GET", ["set"]) => self.set_page(g, q.get("code").map_or("", String::as_str)),
                    ("GET", ["missing"]) => self.missing(g, q.get("set").map_or("", String::as_str)),
                    ("GET", ["search"]) => self.search(g, &q),
                    ("GET", ["autocomplete"]) => {
                        let names = g.game.autocomplete(q.get("q").map_or("", String::as_str)).map_err(upstream)?;
                        Ok(Reply::Json(json!({ "names": names })))
                    }
                    ("POST", ["import"]) if q.get("preview").is_some_and(|v| v == "1") => self.preview_import(g, req),
                    ("POST", ["import"]) => self.import(g, req),
                    ("POST", ["import", "commit"]) => self.commit_import(g, body),
                    ("GET", ["jobs"]) => Ok(self.jobs(g)),
                    ("POST", ["refresh"]) => self.job(g, "refresh", |r| {
                        let n = g.refresh_prices(&mut |done, total| r.report(done, total, "Refreshing prices"))?;
                        Ok(json!({ "updated": n }))
                    }),
                    ("POST", ["price-history"]) => self.job(g, "price-history", |r| {
                        let n = g.load_price_history(&mut |msg| r.report(0, 0, msg))?;
                        Ok(json!({ "printings": n }))
                    }),
                    ("GET", ["export"]) => self.export(g, &q),
                    ("POST", ["export"]) => self.save_export(g, &q),
                    ("POST", ["scan"]) => self.scan(g, body),
                    ("POST", ["scan", "frame"]) => self.scan_frame(g, body),
                    _ => Err(not_found(format!("no {m} /api/{path}"))),
                }
            }
            _ => Err(not_found(format!("no {m} /api/{path}"))),
        }
    }

    /// Runs a long operation unless one of the same kind is already running.
    /// Runs a long operation unless one of the same kind is already running. `run` can
    /// report its progress, which `GET .../jobs` gives while it runs.
    /// Lets links to other sites open in the system's browser (`POST /api/open`).
    pub fn with_url_opener(mut self, open: OpenUrl) -> Api {
        self.open_url = Some(open);
        self
    }

    /// Opens `{"url"}` (http or https only) in the system's browser.
    fn open_link(&self, body: &[u8]) -> Result {
        #[derive(Deserialize)]
        struct Req {
            url: String,
        }
        let req: Req = decode(body)?;
        let open = self
            .open_url
            .as_ref()
            .ok_or_else(|| Error { status: StatusCode::NOT_IMPLEMENTED, message: "links open in this browser here".into() })?;
        let url = url::Url::parse(req.url.trim()).map_err(|_| bad("that isn't a web address"))?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(bad("only http and https links can be opened"));
        }
        open(url.as_str()).map_err(|e| internal(e.context("opening the link")))?;
        tracing::info!(host = url.host_str().unwrap_or(""), "link opened in the browser");
        Ok(Reply::Json(json!({ "opened": true })))
    }

    fn job(&self, g: &GameService, name: &str, run: impl FnOnce(&Reporter) -> anyhow::Result<Value>) -> Result {
        let key = format!("{}/{name}", g.game.id());
        {
            let mut busy = self.busy.lock().unwrap();
            if busy.contains_key(&key) {
                return Err(Error { status: StatusCode::CONFLICT, message: "already running".into() });
            }
            busy.insert(key.clone(), JobProgress::default());
        }
        let res = run(&Reporter { api: self, key: key.clone() });
        self.busy.lock().unwrap().remove(&key);
        res.map(Reply::Json).map_err(upstream)
    }

    /// The game's long jobs running now, with their progress.
    fn jobs(&self, g: &GameService) -> Reply {
        let prefix = format!("{}/", g.game.id());
        let busy = self.busy.lock().unwrap();
        let jobs: Vec<Value> = busy
            .iter()
            .filter_map(|(k, p)| {
                k.strip_prefix(&prefix).map(|name| json!({ "name": name, "done": p.done, "total": p.total, "message": p.message }))
            })
            .collect();
        Reply::Json(json!({ "jobs": jobs }))
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
            "client": self.client.as_str(), "saveExports": self.export_dir.is_some(), "opensLinks": self.open_url.is_some(),
        }))
    }

    /// A number that changes whenever any collection does, so open pages can refresh
    /// after background work (such as the daily price refresh) or after another device
    /// changed the files: each game's files are checked for changes first.
    fn status(&self) -> Reply {
        for g in &self.games {
            g.refresh_files();
        }
        let revision: u64 = self.games.iter().map(|g| g.revision()).sum();
        Reply::Json(json!({ "revision": revision }))
    }

    fn home(&self, g: &GameService, range: &str) -> Reply {
        let cards = g.store.cards();
        let st = compute_stats(&cards, &g.cached_sets(), self.main_set_only());
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
    fn statistics(&self, g: &GameService) -> Reply {
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

    fn filters(&self, g: &GameService) -> Reply {
        let sets: Vec<Value> = set_options(&g.store.cards()).into_iter().map(|o| json!({ "Code": o.code, "Label": o.label })).collect();
        let extra: Vec<Value> = g.game.filters().iter().map(|f| json!({ "Name": f.name, "Options": f.options })).collect();
        let (text, set) = g.game.search_hints();
        Reply::Json(json!({
            "sets": sets, "rarities": g.game.rarities(), "finishes": g.game.finishes(), "finishNames": finish_names(g.game.as_ref()),
            "filters": extra, "conditions": choices::CONDITIONS, "searchHints": [text, set], "tags": g.store.tag_names(),
            "languages": g.game.languages().iter().map(|(code, name)| json!({ "code": code, "name": name })).collect::<Vec<_>>(),
        }))
    }

    /// The filtered cards, sorted, with counts and money totals. Filters: q, set,
    /// rarity, finish, condition, language, card_type, lot, has_note, price_min,
    /// price_max, acquired_from, acquired_to, min_copies, no_price_paid, unpriced,
    /// tags, tags_mode (all or any), tags_not, untagged, and one per game filter,
    /// named f.<filter name>. Sorting: sort (a [`SortKey`] name)
    /// and dir (asc or desc); without sort, the Card order setting.
    fn cards(&self, g: &GameService, params: &HashMap<String, String>) -> Result {
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
    fn filtered_cards(&self, g: &GameService, params: &HashMap<String, String>) -> Result<(Vec<Card>, SortKey, bool)> {
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
        let q = Query {
            text: get("q"),
            set: get("set"),
            rarity: get("rarity"),
            finish: get("finish"),
            condition: get("condition"),
            language: get("language"),
            card_type: get("card_type"),
            lot: get("lot"),
            has_note: flag("has_note"),
            price_min: number("price_min")?,
            price_max: number("price_max")?,
            acquired_from: date("acquired_from")?,
            acquired_to: date("acquired_to")?,
            min_copies: number("min_copies")?.map(|n| n as i64),
            no_price_paid: flag("no_price_paid"),
            unpriced: flag("unpriced"),
            matches: Some(Box::new(|c: &Card| {
                filters.iter().all(|f| params.get(&format!("f.{}", f.name)).is_none_or(|opt| is_any(opt) || (f.matches)(c, opt)))
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

    /// Every tag in use with its totals over the copies that carry it, and the
    /// untagged copies' totals (null when every copy is tagged).
    fn tags(&self, g: &GameService) -> Reply {
        Reply::Json(json!(tag_summaries(&g.store.cards())))
    }

    /// Renames a tag everywhere: `{"name": "<new>"}`. Returns `changed` and `undo`.
    fn rename_tag(&self, g: &GameService, name: &str, body: &[u8]) -> Result {
        #[derive(Deserialize)]
        struct Req {
            name: String,
        }
        let req: Req = decode(body)?;
        let to = clean_tag(&req.name)
            .ok_or_else(|| bad(format!("{:?} can't be a tag: at most 40 characters, with no comma or semicolon", req.name.trim())))?;
        self.tag_in_use(g, name)?;
        let edit = g.store.rename_tag(name, &to).map_err(internal)?;
        tracing::info!(game = g.game.id(), from = name, to, changed = edit.changed, "tag renamed");
        Ok(Reply::Json(tag_edit_answer(g, &edit)))
    }

    /// Removes a tag from every purchase. Returns `changed` and `undo`.
    fn delete_tag(&self, g: &GameService, name: &str) -> Result {
        self.tag_in_use(g, name)?;
        let edit = g.store.delete_tag(name).map_err(internal)?;
        tracing::info!(game = g.game.id(), tag = name, changed = edit.changed, "tag deleted");
        Ok(Reply::Json(tag_edit_answer(g, &edit)))
    }

    fn tag_in_use(&self, g: &GameService, name: &str) -> Result<()> {
        if g.store.tag_names().iter().any(|t| same_tag(t, name)) {
            Ok(())
        } else {
            Err(not_found(format!("no card has the tag {:?}", name)))
        }
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
    fn main_set_only(&self) -> bool {
        self.settings.get().set_completion != "all"
    }

    /// The set's cards not owned, by collector number, following the Set completion
    /// setting, with `totalCost`: one of each at market price, in the usual finish
    /// where it has a price. `printedSize` is the printed run used, or null for all.
    fn missing(&self, g: &GameService, code: &str) -> Result {
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

    /// Every printing of the card in a language (the card's own when `lang` is "").
    fn printings(&self, g: &GameService, key: &str, lang: &str) -> Result {
        let c = self.find(g, key)?;
        let lang = if lang.is_empty() { c.language.as_str() } else { lang };
        let found = g.game.printings(&c, lang).map_err(upstream)?;
        Ok(Reply::Json(json!({ "current": c.id, "printings": views(&found) })))
    }

    /// Searches every game's collection.
    fn search_all(&self, q: &str) -> Reply {
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
    fn change_printing(&self, g: &GameService, key: &str, body: &[u8]) -> Result {
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

    fn bulk_edit(&self, g: &GameService, body: &[u8]) -> Result {
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Req {
            keys: Vec<String>,
            condition: String,
            finish: String,
            price_paid: Option<f64>,
            acquired_date: String,
            /// `null` leaves notes as they are; "" clears them.
            note: Option<String>,
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
        let change = BulkChange {
            condition: (!req.condition.is_empty()).then(|| Condition::parse(&req.condition)),
            finish: (!req.finish.is_empty()).then_some(req.finish),
            price_paid: req.price_paid,
            acquired_date: (!req.acquired_date.is_empty()).then_some(req.acquired_date),
            note: req.note,
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

    /// Replaces the collection with a backup. With `ifRevision` (Undo), only while the
    /// collection is still at that revision: otherwise something changed since, and
    /// the undo would lose it.
    fn restore_backup(&self, g: &GameService, body: &[u8]) -> Result {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Req {
            name: String,
            if_revision: Option<u64>,
        }
        let req: Req = decode(body)?;
        if req.if_revision.is_some_and(|r| r != g.store.revision()) {
            return Err(Error {
                status: StatusCode::CONFLICT,
                message: "the collection has changed since, so it can't be undone here; use Tools → Restore a backup".into(),
            });
        }
        if !g.store.backups().map_err(internal)?.iter().any(|b| b.name == req.name) {
            return Err(not_found(format!("there is no backup named {:?}", req.name)));
        }
        let backup = g.store.restore(&req.name).map_err(internal)?;
        Ok(Reply::Json(json!({ "cards": g.store.len(), "undo": undo(g, &backup) })))
    }

    /// The file sent as a multipart "file" field or as the raw body.
    fn upload(req: &Request) -> Result<Vec<u8>> {
        let content_type = req.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("");
        if content_type.starts_with("multipart/") {
            multipart_file(content_type, req.body()).ok_or_else(|| bad("no \"file\" in the upload"))
        } else {
            Ok(req.body().clone())
        }
    }

    /// Imports the file sent as a multipart "file" field or as the raw body.
    fn import(&self, g: &GameService, req: &Request) -> Result {
        let data = Self::upload(req)?;
        self.job(g, "import", |r| Ok(import_result(g, &g.import(&data, &mut |done, total| r.report(done, total, "Matching cards"))?)))
    }

    /// Reads and matches an import file without saving it, and keeps the result for
    /// `import/commit`. A new preview replaces the game's previous one.
    fn preview_import(&self, g: &GameService, req: &Request) -> Result {
        let data = Self::upload(req)?;
        self.job(g, "import", |r| {
            let preview = g.preview_import(&data, &mut |done, total| r.report(done, total, "Matching cards"))?;
            let token = new_token();
            let answer = json!({
                "token": token, "format": preview.report.format, "rows": preview.report.rows,
                "cards": preview.printings(), "copies": preview.copies, "newCards": preview.new_cards,
                "alreadyOwned": preview.already_owned, "pricePaid": round_cents(preview.price_paid),
                "notes": preview.report.notes, "unmatched": preview.report.unmatched, "header": preview.report.header,
            });
            let pending = PendingImport { token, made: Instant::now(), preview };
            self.previews.lock().unwrap().insert(g.game.id().to_string(), pending);
            Ok(answer)
        })
    }

    /// Saves the game's import preview, if `token` names it and it isn't too old.
    fn commit_import(&self, g: &GameService, body: &[u8]) -> Result {
        #[derive(Deserialize)]
        struct Req {
            token: String,
            /// Import the cards into this lot (a pasted list or a CSV of its contents).
            #[serde(default)]
            lot: String,
            /// Tags added to every purchase in the import.
            #[serde(default)]
            tags: Vec<String>,
        }
        let req: Req = decode(body)?;
        let tags = tags_from(&req.tags)?;
        let lot = match req.lot.as_str() {
            "" => None,
            id => Some(self.open_lot(g, id)?),
        };
        let pending = {
            let mut previews = self.previews.lock().unwrap();
            previews.retain(|_, p| p.made.elapsed() < PREVIEW_LIFETIME);
            match previews.get(g.game.id()) {
                Some(p) if p.token == req.token => previews.remove(g.game.id()),
                _ => None,
            }
        };
        let pending = pending.ok_or_else(|| Error {
            status: StatusCode::GONE,
            message: "this preview is no longer available; preview the file again".into(),
        })?;
        let mut preview = pending.preview;
        if let Some(lot) = &lot {
            lots::put_in_lot(&mut preview.cards, lot);
        }
        tag_every_purchase(&mut preview.cards, &tags).map_err(|e| bad(format!("{e:#}")))?;
        self.job(g, "import", |r| {
            r.report(0, 0, "Saving");
            let res = g.commit_import(preview)?;
            let mut answer = import_result(g, &res);
            if let Some(lot) = &lot {
                g.reprice_lot(lot)?;
                answer["lot"] = lots::summary(g, lot);
            }
            Ok(answer)
        })
    }

    /// An export for `GET`/`POST /export`: `format=json` (the default) is the whole
    /// collection file, which import reads back; `csv` and `report` (a printable HTML
    /// valuation) take the same filter and sort parameters as `GET /cards`.
    fn export_file(&self, g: &GameService, params: &HashMap<String, String>) -> Result<(String, &'static str, Vec<u8>)> {
        let stem = format!("{}-collection-{}", g.game.id(), dates::today_string());
        match params.get("format").map_or("json", |f| f.as_str()) {
            "json" => {
                let data = std::fs::read(g.store.file_path()).map_err(|e| internal(e.into()))?;
                Ok((format!("{stem}.json"), "application/json", data))
            }
            "csv" => {
                let (cards, _, _) = self.filtered_cards(g, params)?;
                let data = write_csv(&cards, g.game.catalog_id_label()).map_err(internal)?;
                Ok((format!("{stem}.csv"), "text/csv; charset=utf-8", data))
            }
            "report" => {
                let (cards, _, _) = self.filtered_cards(g, params)?;
                let filtered = params.keys().any(|k| !matches!(k.as_str(), "format" | "sort" | "dir"));
                let html = report::valuation(g, &cards, filtered);
                Ok((format!("{}-valuation-{}.html", g.game.id(), dates::today_string()), "text/html; charset=utf-8", html.into_bytes()))
            }
            _ => Err(bad("format must be json, csv or report")),
        }
    }

    /// Downloads an export (see `export_file`).
    fn export(&self, g: &GameService, params: &HashMap<String, String>) -> Result {
        let (name, content_type, data) = self.export_file(g, params)?;
        Ok(Reply::File { name, content_type, data })
    }

    /// Saves an export (see `export_file`) to the export folder.
    fn save_export(&self, g: &GameService, params: &HashMap<String, String>) -> Result {
        let dir = self.export_dir.as_ref().ok_or_else(|| not_found("this app downloads exports instead (GET)"))?;
        let (name, _, data) = self.export_file(g, params)?;
        let path = dir.join(name);
        std::fs::create_dir_all(dir).and_then(|_| std::fs::write(&path, data)).map_err(|e| internal(e.into()))?;
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
        let data_dir = match &self.folders {
            Some(f) => f.data.display().to_string(),
            None => self.settings.path().and_then(|p| p.parent()).map(|d| d.display().to_string()).unwrap_or_default(),
        };
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
    note: String,
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
        tags_from(&self.tags)?;
        check_date(&self.acquired_date)
    }

    /// Adds the purchase to `c` (in the printing's first finish if none is given).
    fn add(&self, c: &mut Card) {
        let finish = if self.finish.is_empty() { c.finishes.first().cloned().unwrap_or_default() } else { self.finish.clone() };
        let p = Purchase {
            quantity: self.quantity,
            price_paid: self.price_paid,
            acquired_date: self.acquired_date.clone(),
            note: self.note.trim().to_string(),
            lot: self.lot.clone(),
            tags: normalize_tags(&self.tags).unwrap_or_default(), // checked by validate
        };
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
