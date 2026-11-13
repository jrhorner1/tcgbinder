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

/// Talks to the Scryfall API while respecting its published rate limits
/// (<https://scryfall.com/docs/api/rate-limits>): at most ~10 requests a second overall
/// and 2 a second for the collection and search endpoints.
pub struct Client {
    pub base_url: String,
    /// The minimum gap between ordinary requests.
    pub fast_gap: Duration,
    /// The minimum gap between search and /cards/collection requests.
    pub slow_gap: Duration,
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
            http: OnceLock::new(),
            last: Mutex::new((None, None)),
        }
    }

    /// A client without rate limits, for tests against a fake server.
    pub fn unlimited(base_url: &str) -> Client {
        Client { fast_gap: Duration::ZERO, slow_gap: Duration::ZERO, ..Client::new(base_url) }
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

    /// Performs a request and decodes a 200 answer. Rate limits and server errors are
    /// retried; other errors carry their status as an [`HttpError`], so callers can
    /// treat 404 as "not found".
    pub fn request<T: DeserializeOwned>(&self, method: Method, path: &str, body: Option<&Value>, slow: bool) -> Result<T> {
        let url = format!("{}{path}", self.base_url);
        let mut attempt = 0;
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
            if (status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()) && attempt < 3 {
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
        self.request(Method::GET, path, None, false)
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
        match self.request(Method::POST, "/cards/collection", Some(&body), true) {
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

    #[test]
    fn uuids() {
        assert!(is_uuid("0afa0e33-4804-4b00-b625-c2d6b61090fc"));
        assert!(!is_uuid("0afa0e33-4804-4b00-b625-c2d6b61090f"));
        assert!(!is_uuid("0afa0e33x4804-4b00-b625-c2d6b61090fc"));
        assert_eq!(escape("a b/c"), "a%20b%2Fc");
    }
}
