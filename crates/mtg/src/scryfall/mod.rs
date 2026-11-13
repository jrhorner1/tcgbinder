//! A small, rate-limited client for the Scryfall API (<https://scryfall.com/docs/api>):
//! card lookup, search, set listings and set metadata.

mod card;
mod search;
mod sets;

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use reqwest::blocking::Client as HttpClient;
use reqwest::{Method, StatusCode};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};

pub use card::{Card, Face, ImageUris, Prices};
pub use search::{SearchPage, SearchResponse, build_search_query};
pub use sets::load_sets;

/// The User-Agent sent to every service, as Scryfall asks.
pub const USER_AGENT: &str = concat!("TCGBinder/", env!("CARGO_PKG_VERSION"), " (+https://github.com/jrhorner1/tcgbinder)");

/// The most identifiers /cards/collection takes at once.
pub const COLLECTION_BATCH_SIZE: usize = 75;

/// An HTTP error answer from Scryfall.
#[derive(Debug)]
pub struct HttpError {
    pub status: u16,
    pub message: String,
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for HttpError {}

/// The HTTP status of a failed request, if it got an answer.
pub fn status_of(e: &anyhow::Error) -> Option<u16> {
    e.downcast_ref::<HttpError>().map(|e| e.status)
}

/// The paths Scryfall limits to 2 requests a second; every other path allows 10.
const SLOW_PATHS: &[&str] = &["/cards/search", "/cards/named", "/cards/random", "/cards/collection"];

/// Whether a request to `path` falls under Scryfall's 2-a-second limit.
fn is_slow_path(path: &str) -> bool {
    SLOW_PATHS.iter().any(|p| path.strip_prefix(p).is_some_and(|rest| rest.is_empty() || rest.starts_with(['?', '/'])))
}

/// Talks to the Scryfall API while respecting its published rate limits
/// (<https://scryfall.com/docs/api/rate-limits>): 2 requests a second for search,
/// named, random and collection lookups, and about 10 a second for everything else.
pub struct Client {
    pub base_url: String,
    /// The minimum gap between ordinary requests.
    pub fast_gap: Duration,
    /// The minimum gap between requests to the 2-a-second paths ([`SLOW_PATHS`]).
    pub slow_gap: Duration,
    /// How long to wait after an HTTP 429 before the one retry. Scryfall limits access
    /// for 30 seconds after one; a longer `Retry-After` header wins.
    pub limited_wait: Duration,
    http: OnceLock<HttpClient>,
    /// The times of the last request and the last slow one.
    last: Mutex<(Option<Instant>, Option<Instant>)>,
}

impl Default for Client {
    fn default() -> Self {
        Client::new("https://api.scryfall.com")
    }
}

impl Client {
    /// A client for the API at `base_url`.
    pub fn new(base_url: &str) -> Client {
        Client {
            base_url: base_url.trim_end_matches('/').to_string(),
            fast_gap: Duration::from_millis(110),
            slow_gap: Duration::from_millis(550),
            limited_wait: Duration::from_secs(30),
            http: OnceLock::new(),
            last: Mutex::new((None, None)),
        }
    }

    /// A client without rate limits, for tests against a fake server.
    pub fn unlimited(base_url: &str) -> Client {
        Client { fast_gap: Duration::ZERO, slow_gap: Duration::ZERO, limited_wait: Duration::ZERO, ..Client::new(base_url) }
    }

    // Built on first use: the blocking client can't be created on an async runtime's
    // threads, and requests only ever run on worker threads.
    fn http(&self) -> &HttpClient {
        self.http
            .get_or_init(|| HttpClient::builder().user_agent(USER_AGENT).timeout(Duration::from_secs(30)).build().expect("HTTP client"))
    }

    fn wait(&self, slow: bool) {
        let mut last = self.last.lock().unwrap();
        let mut next = last.0.map(|t| t + self.fast_gap);
        if slow && let Some(s) = last.1.map(|t| t + self.slow_gap) {
            next = Some(next.map_or(s, |n| n.max(s)));
        }
        if let Some(next) = next {
            let now = Instant::now();
            if next > now {
                std::thread::sleep(next - now);
            }
        }
        let now = Instant::now();
        last.0 = Some(now);
        if slow {
            last.1 = Some(now);
        }
    }

    /// How long to wait after a 429: [`Client::limited_wait`], or the `Retry-After`
    /// header's seconds if longer.
    fn wait_after_limit(&self, retry_after: Option<&str>) -> Duration {
        let header = retry_after.and_then(|v| v.trim().parse::<u64>().ok()).map_or(Duration::ZERO, Duration::from_secs);
        self.limited_wait.max(header)
    }

    /// Performs a request and decodes a 200 answer. The gap before it depends on the
    /// path ([`SLOW_PATHS`]). Server and network errors are retried after short waits;
    /// a 429 is retried once, after [`Client::wait_after_limit`]. Other errors carry
    /// their status as an [`HttpError`], so callers can treat 404 as "not found".
    pub fn request<T: DeserializeOwned>(&self, method: Method, path: &str, body: Option<&Value>) -> Result<T> {
        let url = format!("{}{path}", self.base_url);
        let slow = is_slow_path(path);
        let mut attempt = 0;
        let mut limited = false;
        loop {
            self.wait(slow);
            let mut req = self.http().request(method.clone(), &url).header("Accept", "application/json");
            if let Some(b) = body {
                req = req.json(b);
            }
            let resp = match req.send() {
                Ok(r) => r,
                Err(e) if attempt < 2 => {
                    tracing::debug!(error = %e, "scryfall request failed; retrying");
                    attempt += 1;
                    std::thread::sleep(Duration::from_secs(1));
                    continue;
                }
                Err(e) => return Err(e).with_context(|| format!("scryfall {method} {path}")),
            };
            let status = resp.status();
            if status == StatusCode::TOO_MANY_REQUESTS {
                if limited {
                    return Err(HttpError {
                        status: status.as_u16(),
                        message: "Scryfall is limiting requests from this app; try again in a minute".into(),
                    }
                    .into());
                }
                limited = true;
                let wait = self.wait_after_limit(resp.headers().get("retry-after").and_then(|v| v.to_str().ok()));
                tracing::warn!(path, wait_secs = wait.as_secs(), "scryfall is limiting requests; waiting before one retry");
                std::thread::sleep(wait);
                continue;
            }
            if status.is_server_error() && attempt < 3 {
                attempt += 1;
                std::thread::sleep(Duration::from_secs(attempt));
                continue;
            }
            if status != StatusCode::OK {
                let text = resp.text().unwrap_or_default();
                let text: String = text.chars().take(512).collect();
                return Err(
                    HttpError { status: status.as_u16(), message: format!("scryfall {method} {path}: {status} {}", text.trim()) }.into()
                );
            }
            return resp.json().with_context(|| format!("scryfall {method} {path}: reading the answer"));
        }
    }

    fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.request(Method::GET, path, None)
    }

    /// Like [`Client::get`], with a 404 answer as `None`.
    fn get_found<T: DeserializeOwned>(&self, path: &str) -> Result<Option<T>> {
        match self.get(path) {
            Ok(v) => Ok(Some(v)),
            Err(e) if status_of(&e) == Some(404) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// One printing by Scryfall ID.
    pub fn card(&self, id: &str) -> Result<Card> {
        anyhow::ensure!(is_uuid(id), "{id:?} is not a Scryfall ID");
        self.get(&format!("/cards/{id}"))
    }

    /// The printing with a set code and collector number, in a language (Scryfall's
    /// code, e.g. "ja"; "" for English). `None` if there is none.
    pub fn card_by_number(&self, set: &str, number: &str, lang: &str) -> Result<Option<Card>> {
        let mut path = format!("/cards/{}/{}", escape(&set.to_lowercase()), escape(number));
        if !lang.is_empty() && lang != "en" {
            path = format!("{path}/{}", escape(lang));
        }
        self.get_found(&path)
    }

    /// The printing with a TCGplayer product ID.
    pub fn card_by_tcgplayer(&self, id: i64) -> Result<Option<Card>> {
        self.get_found(&format!("/cards/tcgplayer/{id}"))
    }

    /// A card by an approximate name, tolerating typos (Scryfall's
    /// /cards/named?fuzzy=). `None` when nothing (or more than one card) matches.
    pub fn named_fuzzy(&self, name: &str) -> Result<Option<Card>> {
        self.get_found(&format!("/cards/named?fuzzy={}", url::form_urlencoded::byte_serialize(name.as_bytes()).collect::<String>()))
    }

    /// Calls /cards/collection. Scryfall rejects a whole batch with 400 if any one
    /// identifier is malformed, so on 400 the batch is split to isolate the bad
    /// identifier and the rest still resolve.
    pub fn fetch_collection(&self, ids: &[Map<String, Value>]) -> Result<CollectionResponse> {
        let body = json!({ "identifiers": ids });
        match self.request(Method::POST, "/cards/collection", Some(&body)) {
            Err(e) if status_of(&e) == Some(400) => {
                if ids.len() == 1 {
                    tracing::warn!(identifier = ?ids[0], error = %e, "scryfall rejected identifier");
                    return Ok(CollectionResponse { data: Vec::new(), not_found: ids.to_vec() });
                }
                let (left, right) = ids.split_at(ids.len() / 2);
                let mut out = self.fetch_collection(left)?;
                let more = self.fetch_collection(right)?;
                out.data.extend(more.data);
                out.not_found.extend(more.not_found);
                Ok(out)
            }
            other => other,
        }
    }
}

/// The answer to /cards/collection.
#[derive(Clone, Debug, Default, Deserialize, serde::Serialize)]
#[serde(default)]
pub struct CollectionResponse {
    pub data: Vec<Card>,
    pub not_found: Vec<Map<String, Value>>,
}

/// Whether `s` is a UUID, the shape of Scryfall IDs.
pub fn is_uuid(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_hexdigit(),
        })
}

/// Escapes a path segment.
pub(crate) fn escape(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect::<String>().replace('+', "%20")
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::testing::{FakeServer, Reply};

    #[test]
    fn slow_paths() {
        assert!(is_slow_path("/cards/search?q=bolt"));
        assert!(is_slow_path("/cards/named?fuzzy=bolt"));
        assert!(is_slow_path("/cards/collection"));
        assert!(is_slow_path("/cards/random"));
        assert!(!is_slow_path("/cards/m19/156"));
        assert!(!is_slow_path("/cards/autocomplete?q=sol"));
        assert!(!is_slow_path("/cards/searching"));
        assert!(!is_slow_path("/sets"));
    }

    /// A server answering 429 to the first `limited` requests and a card after that.
    fn limited_server(limited: usize) -> FakeServer {
        let count = std::sync::atomic::AtomicUsize::new(0);
        FakeServer::start(move |_| {
            if count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) < limited {
                Reply::status(429, r#"{"object":"error","code":"rate_limited"}"#)
            } else {
                Reply::json(&json!({"id": "0afa0e33-4804-4b00-b625-c2d6b61090fc", "name": "Shock"}))
            }
        })
    }

    #[test]
    fn one_429_is_retried() {
        let srv = limited_server(1);
        let card = Client::unlimited(&srv.url).card("0afa0e33-4804-4b00-b625-c2d6b61090fc").unwrap();
        assert_eq!(card.name, "Shock");
        assert_eq!(srv.calls.lock().unwrap().len(), 2);
    }

    #[test]
    fn two_429s_fail() {
        let srv = limited_server(2);
        let err = Client::unlimited(&srv.url).card("0afa0e33-4804-4b00-b625-c2d6b61090fc").unwrap_err();
        assert_eq!(status_of(&err), Some(429));
        assert!(err.to_string().contains("limiting requests"), "{err}");
        assert_eq!(srv.calls.lock().unwrap().len(), 2);
    }

    #[test]
    fn wait_after_limit() {
        let c = Client::new("http://localhost");
        assert_eq!(c.wait_after_limit(None), Duration::from_secs(30));
        assert_eq!(c.wait_after_limit(Some("5")), Duration::from_secs(30));
        assert_eq!(c.wait_after_limit(Some("60")), Duration::from_secs(60));
        assert_eq!(c.wait_after_limit(Some("soon")), Duration::from_secs(30));
    }

    #[test]
    fn named_waits_the_slow_gap() {
        let srv = limited_server(0);
        let gap = Duration::from_millis(300);
        let c = Client { slow_gap: gap, ..Client::unlimited(&srv.url) };
        let start = Instant::now();
        c.card("0afa0e33-4804-4b00-b625-c2d6b61090fc").unwrap();
        c.card("0afa0e33-4804-4b00-b625-c2d6b61090fc").unwrap();
        assert!(start.elapsed() < gap, "ordinary lookups don't wait the slow gap");
        let start = Instant::now();
        c.named_fuzzy("shock").unwrap();
        c.named_fuzzy("shock").unwrap();
        assert!(start.elapsed() >= gap, "the second /cards/named waits");
    }

    #[test]
    fn uuids() {
        assert!(is_uuid("0afa0e33-4804-4b00-b625-c2d6b61090fc"));
        assert!(!is_uuid("0afa0e33-4804-4b00-b625-c2d6b61090f"));
        assert!(!is_uuid("0afa0e33x4804-4b00-b625-c2d6b61090fc"));
        assert_eq!(escape("a b/c"), "a%20b%2Fc");
    }
}
