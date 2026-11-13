//! Fake HTTP servers for tests (with the `testing` feature, for other crates' tests
//! too): a handler answers each request, and every request is recorded.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A request as the handler sees it.
pub struct Request {
    pub path: String,
    pub query: HashMap<String, String>,
    pub body: Vec<u8>,
}

/// The handler's answer.
pub struct Reply {
    pub status: u16,
    pub body: Vec<u8>,
}

impl Reply {
    pub fn json(v: &impl serde::Serialize) -> Reply {
        Reply { status: 200, body: serde_json::to_vec(v).unwrap() }
    }
    pub fn bytes(b: Vec<u8>) -> Reply {
        Reply { status: 200, body: b }
    }
    pub fn status(status: u16, body: &str) -> Reply {
        Reply { status, body: body.as_bytes().to_vec() }
    }
    pub fn not_found() -> Reply {
        Reply::status(404, r#"{"object":"error","code":"not_found"}"#)
    }
}

/// A running fake server. It stops when the test process ends.
pub struct FakeServer {
    pub url: String,
    /// Every request's path and query, in order.
    pub calls: Arc<Mutex<Vec<String>>>,
}

impl FakeServer {
    pub fn start(handler: impl Fn(&Request) -> Reply + Send + 'static) -> FakeServer {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let url = format!("http://{}", server.server_addr());
        let calls = Arc::new(Mutex::new(Vec::new()));
        let log = calls.clone();
        std::thread::spawn(move || {
            for mut req in server.incoming_requests() {
                let full = req.url().to_string();
                log.lock().unwrap().push(full.clone());
                let (path, qs) = full.split_once('?').unwrap_or((&full, ""));
                let mut body = Vec::new();
                let _ = req.as_reader().read_to_end(&mut body);
                let r = Request {
                    path: url::form_urlencoded::parse(path.as_bytes()).map(|(k, _)| k.into_owned()).next().unwrap_or_default(),
                    query: url::form_urlencoded::parse(qs.as_bytes()).into_owned().collect(),
                    body,
                };
                let reply = handler(&r);
                let _ = req.respond(tiny_http::Response::from_data(reply.body).with_status_code(reply.status));
            }
        });
        FakeServer { url, calls }
    }
}

/// A fake Scryfall for tests of the whole app: /cards/search answers "bolt" with five
/// printings in pages of three (anything else matches nothing, as a 404), "e:<set>"
/// lists that set's cards (Lightning Bolt 1, Opt 60, Shock 156), and "oracleid:..."
/// (every printing of a card) is treated as Lightning Bolt.
pub fn search_server() -> (crate::scryfall::Client, FakeServer) {
    use crate::scryfall::{Card, Prices, SearchResponse};
    let base = Arc::new(Mutex::new(String::new()));
    let url_for_pages = base.clone();
    let srv = FakeServer::start(move |req| {
        if req.path != "/cards/search" {
            return Reply::not_found();
        }
        let mut q = req.query.get("q").cloned().unwrap_or_default();
        if let Some(code) = q.strip_prefix("e:") {
            let data = [
                ("aaaaaaaa-0000-4000-8000-000000000101", "Lightning Bolt", "1"),
                ("aaaaaaaa-0000-4000-8000-000000000060", "Opt", "60"),
                ("aaaaaaaa-0000-4000-8000-000000000001", "Shock", "156"),
            ]
            .iter()
            .map(|(id, name, num)| Card {
                id: id.to_string(),
                name: name.to_string(),
                set: code.to_string(),
                set_name: format!("{} Edition", code.to_uppercase()),
                collector_number: num.to_string(),
                rarity: "common".into(),
                finishes: vec!["nonfoil".into(), "foil".into()],
                prices: Prices { usd: Some("0.25".into()), ..Default::default() },
                ..Default::default()
            })
            .collect::<Vec<_>>();
            return Reply::json(&SearchResponse { total_cards: data.len(), data, ..Default::default() });
        }
        if q.starts_with("oracleid:") || q.contains("\"Lightning Bolt\"") {
            q = "bolt".into();
        }
        if !q.to_lowercase().contains("bolt") {
            return Reply::not_found();
        }
        let sets: Vec<String> = match q.split_once("set:") {
            Some((_, set)) => vec![set.to_string()],
            None => ["2xm", "m10", "m11", "a25", "lea"].iter().map(|s| s.to_string()).collect(),
        };
        let page: usize = req.query.get("page").and_then(|p| p.parse().ok()).unwrap_or(1);
        let (start, end) = ((page - 1) * 3, (page * 3).min(sets.len()));
        let mut resp = SearchResponse { total_cards: sets.len(), ..Default::default() };
        for (i, s) in sets[start..end].iter().enumerate() {
            resp.data.push(Card {
                id: format!("{s}-{i}"),
                name: "Lightning Bolt".into(),
                set: s.clone(),
                set_name: format!("{} Edition", s.to_uppercase()),
                collector_number: (100 + start + i).to_string(),
                rarity: "common".into(),
                released_at: "2020-01-01".into(),
                type_line: "Instant".into(),
                oracle_text: "Lightning Bolt deals 3 damage to any target.".into(),
                finishes: vec!["nonfoil".into(), "foil".into()],
                prices: Prices { usd: Some("1.50".into()), ..Default::default() },
                ..Default::default()
            });
        }
        if end < sets.len() {
            resp.has_more = true;
            let mut params: Vec<(String, String)> = req.query.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            params.retain(|(k, _)| k != "page");
            params.push(("page".into(), (page + 1).to_string()));
            let qs = url::form_urlencoded::Serializer::new(String::new()).extend_pairs(params).finish();
            resp.next_page = format!("{}/cards/search?{qs}", url_for_pages.lock().unwrap());
        }
        Reply::json(&resp)
    });
    *base.lock().unwrap() = srv.url.clone();
    (crate::scryfall::Client::unlimited(&srv.url), srv)
}
