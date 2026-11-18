//! TCG Binder's JSON API and the browser UI, as one request handler. Every front end
//! uses it: the desktop and iPhone apps call it in-process through a custom URL scheme,
//! and `tcgbinder --web` serves it over HTTP. The UI (the `ui` folder) is embedded in
//! release builds and read from disk in debug builds.
//!
//! See docs/web-api.md for the endpoints.

mod catalog;
mod collection;
mod folders;
mod home;
mod lists;
mod lots;
mod report;
mod sales;
mod sealed;
mod settings;
mod tags;
mod transfer;
mod views;
mod watchlist;

pub use folders::{Folders, PickFolder, Restart};
use transfer::PendingImport;
pub use transfer::{JobProgress, Reporter};

/// Opens a web page in the system's browser (the apps' web views can't open new
/// windows themselves).
pub type OpenUrl = Arc<dyn Fn(&str) -> anyhow::Result<()> + Send + Sync>;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use http::{HeaderValue, Method, StatusCode, header};
use serde::Deserialize;
use serde_json::{Value, json};

use tcgbinder_core::collection::normalize_user_tags;
use tcgbinder_core::dates;
use tcgbinder_core::scan::Reading;
use tcgbinder_core::service::GameService;
use tcgbinder_core::settings::SettingsStore;

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

fn check_date(date: &str) -> Result<()> {
    if date.is_empty() || dates::parse(date).is_some() { Ok(()) } else { Err(bad("acquired date must look like 2024-03-15")) }
}

/// Checks and normalizes tags sent by a client: a 400 with the reason if any is
/// invalid (or starts with a system prefix, such as `lot:`).
fn tags_from(raw: &[String]) -> Result<Vec<String>> {
    normalize_user_tags(raw).map_err(|e| bad(e.to_string()))
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

    /// Sends a request under /api/ to its handler. The routes are grouped by the file
    /// their handlers live in, so this reads as the API's table of contents.
    fn route(&self, req: &Request, path: &str) -> Result {
        let q = query_params(req);
        let m = req.method();
        let body = req.body();
        let parts: Vec<&str> = path.trim_end_matches('/').split('/').collect();
        match (m.as_str(), parts.as_slice()) {
            // home.rs
            ("GET", ["info"]) => Ok(self.info()),
            ("GET", ["status"]) => Ok(self.status()),
            ("GET", ["search"]) => Ok(self.search_all(q.get("q").map_or("", String::as_str))),
            // folders.rs
            ("GET", ["data-folder"]) => self.data_folder(),
            ("POST", ["data-folder"]) => self.change_data_folder(body),
            ("POST", ["data-folder", "pick"]) => self.pick_data_folder(),
            ("POST", ["data-folder", "inspect"]) => self.inspect_data_folder(body),
            ("POST", ["data", "delete-all"]) => self.delete_all_data(body),
            // settings.rs
            ("GET", ["settings"]) => Ok(self.get_settings()),
            ("PUT", ["settings"]) => self.put_settings(body),
            ("POST", ["open"]) => self.open_link(body),
            // One game's data, by the file its handlers are in.
            (_, ["games", id, rest @ ..]) => {
                let g = self.games.iter().find(|g| g.game.id() == *id).ok_or_else(|| not_found(format!("unknown game {id:?}")))?;
                let key = q.get("key").map_or("", String::as_str);
                match (m.as_str(), rest) {
                    // home.rs
                    ("GET", ["home"]) => Ok(self.home(g, q.get("range").map_or("", String::as_str))),
                    ("GET", ["filters"]) => Ok(self.filters(g)),
                    ("GET", ["stats"]) => Ok(self.statistics(g)),
                    ("GET", ["set"]) => self.set_page(g, q.get("code").map_or("", String::as_str)),
                    ("GET", ["missing"]) => self.missing(g, q.get("set").map_or("", String::as_str)),
                    // collection.rs
                    ("GET", ["cards"]) => self.cards(g, &q),
                    ("POST", ["cards"]) => self.add_card(g, body),
                    ("GET", ["card"]) => self.card(g, key),
                    ("PUT", ["card"]) => self.update_card(g, key, body),
                    ("DELETE", ["card"]) => self.delete_card(g, key),
                    ("GET", ["printings"]) => self.printings(g, key, q.get("lang").map_or("", String::as_str)),
                    ("PUT", ["card", "printing"]) => self.change_printing(g, key, body),
                    ("POST", ["bulk"]) => self.bulk_edit(g, body),
                    // catalog.rs
                    ("GET", ["search"]) => self.search(g, &q),
                    ("GET", ["autocomplete"]) => {
                        let names = g.game.autocomplete(q.get("q").map_or("", String::as_str)).map_err(upstream)?;
                        Ok(Reply::Json(json!({ "names": names })))
                    }
                    ("POST", ["scan"]) => self.scan(g, body),
                    ("POST", ["scan", "frame"]) => self.scan_frame(g, body),
                    // tags.rs
                    ("GET", ["tags"]) => Ok(self.tags(g)),
                    ("PUT", ["tags"]) => self.rename_tag(g, q.get("name").map_or("", String::as_str), body),
                    ("DELETE", ["tags"]) => self.delete_tag(g, q.get("name").map_or("", String::as_str)),
                    // sales.rs
                    ("POST", ["card", "sell"]) => self.sell(g, key, body),
                    ("GET", ["sales"]) => self.sales(g, &q),
                    ("DELETE", ["sales"]) => self.delete_sale(g, &q),
                    // lots.rs
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
                    // lists.rs
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
                    // watchlist.rs
                    ("GET", ["watchlist"]) => Ok(self.watchlist(g)),
                    ("POST", ["watchlist"]) => self.add_watch(g, body),
                    ("PUT", ["watchlist"]) => self.change_watch(g, &q, body),
                    ("DELETE", ["watchlist"]) => self.delete_watch(g, &q),
                    // sealed.rs
                    ("GET", ["sealed"]) => Ok(self.sealed(g)),
                    ("POST", ["sealed"]) => self.add_sealed(g, body),
                    ("PUT", ["sealed"]) => self.update_sealed(g, &q, body),
                    ("DELETE", ["sealed"]) => self.delete_sealed(g, &q),
                    ("GET", ["sealed", "products"]) => self.sealed_products(g, &q),
                    // transfer.rs (the price jobs run in the service)
                    ("POST", ["import"]) if q.get("preview").is_some_and(|v| v == "1") => self.preview_import(g, req),
                    ("POST", ["import"]) => self.import(g, req),
                    ("POST", ["import", "commit"]) => self.commit_import(g, body),
                    ("GET", ["export"]) => self.export(g, &q),
                    ("POST", ["export"]) => self.save_export(g, &q),
                    ("GET", ["backups"]) => Ok(Reply::Json(json!(g.store.backups().map_err(internal)?))),
                    ("POST", ["backups", "restore"]) => self.restore_backup(g, body),
                    ("GET", ["jobs"]) => Ok(self.jobs(g)),
                    ("POST", ["refresh"]) => self.job(g, "refresh", |r| {
                        let n = g.refresh_prices(&mut |done, total| r.report(done, total, "Refreshing prices"))?;
                        Ok(json!({ "updated": n }))
                    }),
                    ("POST", ["price-history"]) => self.job(g, "price-history", |r| {
                        let n = g.load_price_history(&mut |msg| r.report(0, 0, msg))?;
                        Ok(json!({ "printings": n }))
                    }),
                    // folders.rs
                    ("POST", ["conflicts", "merge"]) => self.merge_conflict(g, body),
                    ("POST", ["conflicts", "discard"]) => self.discard_conflict(g, body),
                    _ => Err(not_found(format!("no {m} /api/{path}"))),
                }
            }
            _ => Err(not_found(format!("no {m} /api/{path}"))),
        }
    }

    /// Lets links to other sites open in the system's browser (`POST /api/open`).
    pub fn with_url_opener(mut self, open: OpenUrl) -> Api {
        self.open_url = Some(open);
        self
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
