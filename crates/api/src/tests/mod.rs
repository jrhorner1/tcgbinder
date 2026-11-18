//! The API's tests: the shared set-up here (an `Api` on a temporary folder with a
//! fake Scryfall, and request helpers), and one file per feature.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use tempfile::TempDir;

use tcgbinder_core::collection::{Card, Condition, Store, card_key};
use tcgbinder_core::import::{APP_FILE_FORMAT, is_app_file};
use tcgbinder_core::scan::{Line, Reading};
use tcgbinder_core::service::GameService;
use tcgbinder_core::{Purchase, settings::SettingsStore};
use tcgbinder_mtg::testing::FakeServer;
use tcgbinder_mtg::{ID, Magic, mtgjson, testing};

use super::*;

mod catalog;
mod collection;
mod folders;
mod home;
mod lists;
mod lots;
mod requests;
mod sales;
mod sealed;
mod settings;
mod tags;
mod transfer;
mod watchlist;

const SHOCK: &str = "aaaaaaaa-0000-4000-8000-000000000001";
const OPT: &str = "aaaaaaaa-0000-4000-8000-000000000002";

struct Fixture {
    api: Api,
    svc: Arc<GameService>,
    _dir: TempDir,
    _scryfall: FakeServer,
}

/// Gives `c` copies in one finish and condition.
fn held(mut c: Card, finish: &str, cond: Condition, p: Purchase) -> Card {
    c.add_purchase(finish, cond, p);
    c
}

fn fixture() -> Fixture {
    let (scryfall, fake) = testing::search_server();
    let g = Magic::with_clients(scryfall, mtgjson::Client::new(&fake.url));
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("collection.json"), ID).unwrap();
    let mut shock = held(
        Card {
            id: SHOCK.into(),
            name: "Shock".into(),
            set: "M19".into(),
            set_name: "Core Set 2019".into(),
            collector_number: "156".into(),
            rarity: "common".into(),
            released_at: "2018-07-13".into(),
            finishes: vec!["nonfoil".into(), "foil".into()],
            language: "en".into(),
            market: BTreeMap::from([("nonfoil".into(), 0.20), ("foil".into(), 1.00)]),
            prices_updated: "2026-11-07".into(),
            type_line: "Instant".into(),
            rules_text: "Shock deals 2 damage.".into(),
            ..Card::default()
        },
        "nonfoil",
        Condition::NearMint,
        Purchase { quantity: 2, price_paid: 0.25, acquired_date: "2018-08-08".into(), ..Default::default() },
    );
    shock.set_attr("colors", vec!["R"]);
    shock.set_attr("mana_cost", "{R}");
    shock.set_attr("oracle_id", "o-shock"); // printings are looked up by Oracle ID
    let mut opt = held(
        Card {
            id: OPT.into(),
            name: "Opt".into(),
            set: "DOM".into(),
            set_name: "Dominaria".into(),
            collector_number: "60".into(),
            rarity: "common".into(),
            released_at: "2018-04-27".into(),
            language: "en".into(),
            type_line: "Instant".into(),
            ..Card::default()
        },
        "foil",
        Condition::LightPlay,
        Purchase { quantity: 1, ..Default::default() },
    );
    opt.set_attr("colors", vec!["U"]);
    store.add_cards(vec![shock, opt]).unwrap();
    let svc = Arc::new(GameService::from_store(Box::new(g), store).unwrap());
    let api = Api::new("TCG Binder", "test", Client::Web, Arc::new(SettingsStore::memory()), vec![svc.clone()]);
    Fixture { api, svc, _dir: dir, _scryfall: fake }
}

fn request(method: &str, path: &str, body: Vec<u8>) -> Request {
    http::Request::builder().method(method).uri(path).header(CLIENT_HEADER, "test").body(body).unwrap()
}

/// Calls the API with a JSON body (when given), returning the status and JSON answer.
fn call(api: &Api, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
    let body = body.map(|b| serde_json::to_vec(&b).unwrap()).unwrap_or_default();
    let resp = api.handle(&request(method, path, body));
    (resp.status().as_u16(), serde_json::from_slice(resp.body()).unwrap_or(Value::Null))
}

fn get(api: &Api, path: &str) -> Value {
    let (status, v) = call(api, "GET", path, None);
    assert_eq!(status, 200, "GET {path}: {v}");
    v
}

fn enc(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

/// An export with EchoMTG's real header and made-up rows: four copies the fake
/// Scryfall knows, a token and a basic land it doesn't.
const ECHOMTG: &[u8] = include_bytes!("../../../mtg/tests/echomtg-export.txt");

fn preview(f: &Fixture) -> Value {
    let resp = f.api.handle(&request("POST", "/api/games/mtg/import?preview=1", ECHOMTG.to_vec()));
    assert_eq!(resp.status(), 200);
    serde_json::from_slice(resp.body()).unwrap()
}

fn commit(f: &Fixture, token: &Value) -> (u16, Value) {
    call(&f.api, "POST", "/api/games/mtg/import/commit", Some(json!({ "token": token })))
}

/// A catalog card for lot tests, with a market price.
fn catalog(id: &str, market: f64) -> Value {
    json!({ "id": id, "name": format!("Card {id}"), "set": "TST", "lang": "en", "rarity": "common", "finishes": ["nonfoil", "foil"], "market": { "nonfoil": market } })
}

fn add_in_lot(f: &Fixture, id: &str, market: f64, quantity: i64, lot: &Value) -> Value {
    let body = json!({ "card": catalog(id, market), "quantity": quantity, "finish": "nonfoil", "condition": "NM", "price_paid": 99, "acquired_date": "2001-01-01", "lot": lot });
    let (status, r) = call(&f.api, "POST", "/api/games/mtg/cards", Some(body));
    assert_eq!(status, 200, "{r}");
    r
}

/// The price paid and date of a card's copies in a lot.
fn lot_purchases(f: &Fixture, id: &str, lot: &Value) -> Vec<(f64, String, i64)> {
    let c = &get(&f.api, &format!("/api/games/mtg/card?key={id}"))["card"];
    c["purchases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["lot"] == *lot)
        .map(|p| (p["price_paid"].as_f64().unwrap(), p["acquired_date"].as_str().unwrap().to_string(), p["quantity"].as_i64().unwrap()))
        .collect()
}
