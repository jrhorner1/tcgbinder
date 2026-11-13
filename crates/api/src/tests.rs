use std::collections::BTreeMap;

use serde_json::{Value, json};
use tempfile::TempDir;

use tcgbinder_core::collection::{Card, Condition, Store, card_key};
use tcgbinder_core::scan::{Line, Reading};
use tcgbinder_core::service::{APP_FILE_FORMAT, GameService, is_app_file};
use tcgbinder_core::{Purchase, settings::SettingsStore};
use tcgbinder_mtg::testing::FakeServer;
use tcgbinder_mtg::{ID, Magic, mtgjson, testing};

use super::*;

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
        Purchase { quantity: 2, price_paid: 0.25, acquired_date: "2018-08-08".into() },
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
    let svc = Arc::new(GameService::from_store(Box::new(g), store));
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

#[test]
fn pages_and_info() {
    let f = fixture();
    for p in ["/", "/app.js", "/app.css", "/icon.png"] {
        let resp = f.api.handle(&request("GET", p, Vec::new()));
        assert_eq!(resp.status(), 200, "{p}");
        assert_eq!(resp.headers()["X-Content-Type-Options"], "nosniff");
    }
    assert_eq!(f.api.handle(&request("GET", "/nope.txt", Vec::new())).status(), 404);
    let info = get(&f.api, "/api/info");
    assert_eq!(info["games"][0]["ID"], "mtg");
    assert_eq!(info["games"][0]["Scan"], false); // no photo reader given
    assert_eq!(info["client"], "web");
    assert_eq!(call(&f.api, "GET", "/api/games/pokemon/home", None).0, 404);
    assert_eq!(call(&f.api, "GET", "/api/nothing", None).0, 404);
}

#[test]
fn home_and_cards() {
    let f = fixture();
    let o = get(&f.api, "/api/games/mtg/home?range=30D");
    assert_eq!((o["cards"].as_i64(), o["marketValue"].as_f64(), o["pricePaid"].as_f64()), (Some(3), Some(0.40), Some(0.50)));
    assert_eq!(o["chart"]["range"], "30D");
    assert_eq!(o["topCards"].as_array().unwrap().len(), 1);
    assert_eq!(o["bySet"][0]["Code"], "M19"); // newest set first
    assert_eq!(o["canLoadHistory"], true);

    let all = get(&f.api, "/api/games/mtg/cards");
    assert_eq!((all["matched"].as_i64(), all["sets"].as_i64(), all["cardCount"].as_i64()), (Some(2), Some(2), Some(3)));
    assert_eq!(all["cards"][0]["name"], "Opt"); // by name
    let red = get(&f.api, "/api/games/mtg/cards?f.Color=R");
    assert_eq!(red["matched"], 1);
    assert_eq!(red["cards"][0]["name"], "Shock");
    assert_eq!(red["cards"][0]["key"], SHOCK);
    assert_eq!(get(&f.api, "/api/games/mtg/cards?f.Color=Any&set=dom")["cards"][0]["name"], "Opt");

    let filters = get(&f.api, "/api/games/mtg/filters");
    assert_eq!(filters["finishNames"]["nonfoil"], "Normal");
    assert_eq!(filters["sets"][0]["Label"], "M19 — Core Set 2019");
    assert_eq!(filters["filters"][0]["Name"], "Color");
}

#[test]
fn edit_and_delete_card() {
    let f = fixture();
    let detail = get(&f.api, &format!("/api/games/mtg/card?key={SHOCK}"));
    assert_eq!(detail["card"]["name"], "Shock");
    assert_eq!(detail["details"][0]["Value"], "{R}");
    assert!(detail["link"]["url"].as_str().unwrap().contains("scryfall.com/card/m19/156"));
    assert_eq!(detail["card"]["purchases"].as_array().unwrap().len(), 1);
    assert_eq!(detail["card"]["copy_count"], 2);

    let (status, upd) = call(
        &f.api,
        "PUT",
        &format!("/api/games/mtg/card?key={SHOCK}"),
        Some(json!({"purchases": [
            {"quantity": 1, "finish": "nonfoil", "condition": "NM", "price_paid": 0.10, "acquired_date": "2018-08-08"},
            {"quantity": 1, "finish": "nonfoil", "condition": "NM", "price_paid": 0.12, "acquired_date": "2019-01-02"},
            {"quantity": 3, "finish": "foil", "condition": "LP", "price_paid": 1.5, "acquired_date": "2020-01-02"},
        ]})),
    );
    assert_eq!((status, upd["key"].as_str()), (200, Some(SHOCK)));
    let c = f.svc.store.get(SHOCK).unwrap();
    assert_eq!(c.copy_count(), 5);
    assert_eq!(c.purchases("nonfoil", Condition::NearMint).len(), 2);
    assert_eq!(c.purchases("foil", Condition::LightPlay).iter().map(|p| p.quantity).sum::<i64>(), 3);

    for bad in [json!({"quantity": 1, "acquired_date": "March"}), json!({"quantity": 1, "finish": "etched"}), json!({"quantity": -1})] {
        let (status, _) = call(&f.api, "PUT", &format!("/api/games/mtg/card?key={SHOCK}"), Some(json!({ "purchases": [bad] })));
        assert_eq!(status, 400, "{bad}");
    }
    assert_eq!(call(&f.api, "DELETE", &format!("/api/games/mtg/card?key={SHOCK}"), None).0, 200);
    assert_eq!(f.svc.store.len(), 1);
    assert_eq!(call(&f.api, "GET", &format!("/api/games/mtg/card?key={SHOCK}"), None).0, 404);
}

#[test]
fn saving_no_copies_deletes() {
    let f = fixture();
    let (status, v) = call(&f.api, "PUT", &format!("/api/games/mtg/card?key={OPT}"), Some(json!({"purchases": []})));
    assert_eq!((status, v["deleted"].as_bool()), (200, Some(true)));
    assert!(f.svc.store.get(OPT).is_none());
}

#[test]
fn search_and_add() {
    let f = fixture();
    let page = get(&f.api, "/api/games/mtg/search?text=bolt");
    assert_eq!((page["cards"].as_array().unwrap().len(), page["total"].as_i64()), (3, Some(5)));
    assert_ne!(page["next"], "");
    let more = get(&f.api, &format!("/api/games/mtg/search?text=bolt&next={}", enc(page["next"].as_str().unwrap())));
    assert_eq!(more["cards"].as_array().unwrap().len(), 2);

    let card = page["cards"][1].clone();
    let (status, added) =
        call(&f.api, "POST", "/api/games/mtg/cards", Some(json!({"card": card, "quantity": 4, "finish": "foil", "condition": "NM"})));
    assert_eq!((status, added["newCards"].as_i64()), (200, Some(1)));
    let bolt = f.svc.store.cards().into_iter().find(|c| c.name == "Lightning Bolt").unwrap();
    assert_eq!((bolt.copy_count(), bolt.finish_copy_count("foil")), (4, 4));
    // Search results say which printings are owned.
    assert_eq!(get(&f.api, "/api/games/mtg/search?text=bolt")["owned"][1], 4);

    let (status, _) = call(&f.api, "POST", "/api/games/mtg/cards", Some(json!({"card": card, "quantity": 0})));
    assert_eq!(status, 400);
}

/// Exports the collection and imports it back: every card merges, doubling the
/// quantities.
#[test]
fn export_import_round_trip() {
    let f = fixture();
    let resp = f.api.handle(&request("GET", "/api/games/mtg/export", Vec::new()));
    assert!(resp.headers()[header::CONTENT_DISPOSITION].to_str().unwrap().contains("attachment"));
    let data = resp.body().clone();
    assert!(is_app_file(&data));

    let mut body =
        b"--XyZ\r\nContent-Disposition: form-data; name=\"file\"; filename=\"backup.json\"\r\nContent-Type: application/json\r\n\r\n"
            .to_vec();
    body.extend_from_slice(&data);
    body.extend_from_slice(b"\r\n--XyZ--\r\n");
    let req = http::Request::builder()
        .method("POST")
        .uri("/api/games/mtg/import")
        .header(CLIENT_HEADER, "test")
        .header(header::CONTENT_TYPE, "multipart/form-data; boundary=XyZ")
        .body(body)
        .unwrap();
    let resp = f.api.handle(&req);
    let res: Value = serde_json::from_slice(resp.body()).unwrap();
    assert_eq!(resp.status(), 200, "{res}");
    assert_eq!((res["cards"].as_i64(), res["newCards"].as_i64(), res["alreadyOwned"].as_i64()), (Some(3), Some(0), Some(2)));
    assert_eq!(res["format"], APP_FILE_FORMAT);
    assert_eq!(f.svc.store.cards().iter().map(Card::copy_count).sum::<i64>(), 6);

    // The raw body works too, and a revision change tells open pages to refresh.
    let before = get(&f.api, "/api/status")["revision"].as_u64().unwrap();
    let (status, _) = call(&f.api, "POST", "/api/games/mtg/import", None);
    assert_eq!(status, 502); // an empty file
    let resp = f.api.handle(&request("POST", "/api/games/mtg/import", data));
    assert_eq!(resp.status(), 200);
    assert!(get(&f.api, "/api/status")["revision"].as_u64().unwrap() > before);

    // Apps that can't download files save the export instead.
    assert_eq!(call(&f.api, "POST", "/api/games/mtg/export", None).0, 404);
    let out = tempfile::tempdir().unwrap();
    let api = Api::new("TCG Binder", "test", Client::Phone, Arc::new(SettingsStore::memory()), vec![f.svc.clone()])
        .with_export_dir(out.path().join("exports"));
    let (status, saved) = call(&api, "POST", "/api/games/mtg/export", None);
    assert_eq!(status, 200);
    assert!(is_app_file(&std::fs::read(saved["path"].as_str().unwrap()).unwrap()));
    assert_eq!(get(&api, "/api/info")["saveExports"], true);
}

#[test]
fn changes_need_the_client_header() {
    let f = fixture();
    // A cross-site form or fetch can't add the header, so it must be refused.
    for (method, path) in [("POST", "/api/games/mtg/cards".to_string()), ("DELETE", format!("/api/games/mtg/card?key={SHOCK}"))] {
        let req = http::Request::builder().method(method).uri(&path).body(b"{}".to_vec()).unwrap();
        assert_eq!(f.api.handle(&req).status(), 403, "{method} {path}");
    }
    assert_eq!(f.svc.store.len(), 2);
}

#[test]
fn local_hosts() {
    for (host, want) in [
        ("localhost:8080", true),
        ("127.0.0.1:8080", true),
        ("[::1]:8080", true),
        ("::1", true),
        ("LOCALHOST", true),
        ("evil.example:8080", false),
        ("attacker.com", false),
        ("localhost.attacker.com", false),
    ] {
        assert_eq!(is_local_host(host), want, "{host}");
    }
}

/// Tokens and emblems have no mana cost, so the game gives no extra detail rows; the
/// API must send empty lists, not null, or the page breaks.
#[test]
fn card_without_details() {
    let f = fixture();
    let v = get(&f.api, &format!("/api/games/mtg/card?key={OPT}"));
    assert_eq!(v["details"], json!([]));
    assert_eq!(v["others"], json!([]));
}

#[test]
fn set_page_and_missing() {
    let f = fixture();
    let set = get(&f.api, "/api/games/mtg/set?code=m19");
    assert_eq!((set["code"].as_str(), set["cards"].as_i64()), (Some("M19"), Some(2)));
    assert_eq!(set["top"][0]["each"], 0.2);
    assert_eq!(set["completion"], -1.0); // the set size isn't loaded
    assert_eq!(call(&f.api, "GET", "/api/games/mtg/set?code=ZZZ", None).0, 404);
    let m = get(&f.api, "/api/games/mtg/missing?set=M19");
    // The fake set has 1, 60 and 156; Shock (156) is owned.
    assert_eq!(m["total"], 3);
    assert_eq!(m["missing"].as_array().unwrap().len(), 2);
    assert_eq!(m["missing"][0]["collector_number"], "1");
}

#[test]
fn change_printing() {
    let f = fixture();
    let p = get(&f.api, &format!("/api/games/mtg/printings?key={SHOCK}"));
    let target = p["printings"][0]["id"].as_str().unwrap().to_string();
    let (status, res) = call(&f.api, "PUT", &format!("/api/games/mtg/card/printing?key={SHOCK}"), Some(json!({ "id": target })));
    assert_eq!(status, 200, "{res}");
    let moved = f.svc.store.get(&target).unwrap();
    assert_eq!(moved.copy_count(), 2);
    assert_eq!(moved.purchase_rows()[0].purchase.price_paid, 0.25);
    let (status, _) = call(
        &f.api,
        "PUT",
        &format!("/api/games/mtg/card/printing?key={}", res["key"].as_str().unwrap()),
        Some(json!({"id": "not-a-printing"})),
    );
    assert_eq!(status, 400);
}

#[test]
fn bulk_edit_and_search_all() {
    let f = fixture();
    let keys: Vec<String> = f.svc.store.cards().iter().map(card_key).collect();
    let (status, res) = call(&f.api, "POST", "/api/games/mtg/bulk", Some(json!({"keys": keys, "condition": "HP", "price_paid": 0.5})));
    assert_eq!((status, res["Updated"].as_i64()), (200, Some(2)));
    for c in f.svc.store.cards() {
        for r in c.purchase_rows() {
            assert_eq!((r.condition, r.purchase.price_paid), (Condition::HeavyPlay, 0.5));
        }
    }
    assert_eq!(call(&f.api, "POST", "/api/games/mtg/bulk", Some(json!({"keys": keys}))).0, 400);
    assert_eq!(call(&f.api, "POST", "/api/games/mtg/bulk", Some(json!({"keys": [], "delete": true}))).0, 400);
    assert_eq!(call(&f.api, "POST", "/api/games/mtg/bulk", Some(json!({"keys": keys, "acquired_date": "soon"}))).0, 400);
    let (status, res) = call(&f.api, "POST", "/api/games/mtg/bulk", Some(json!({"keys": [OPT], "finish": "etched"})));
    assert_eq!(status, 200);
    assert_eq!(res["Skipped"].as_array().unwrap().len(), 0); // no known finishes: anything goes

    let found = get(&f.api, "/api/search?q=shock");
    assert_eq!(found["games"].as_array().unwrap().len(), 1);
    assert_eq!(found["games"][0]["Total"], 1);
}

#[test]
fn settings_api() {
    let f = fixture();
    let got = get(&f.api, "/api/settings");
    assert_eq!(got["settings"]["theme"], "system");
    assert_eq!(got["choices"]["theme"].as_array().unwrap().len(), 3);
    // Only the settings sent change.
    let (status, got) = call(&f.api, "PUT", "/api/settings", Some(json!({"theme": "dark", "show_images": false})));
    assert_eq!(status, 200);
    assert_eq!((got["settings"]["theme"].as_str(), got["settings"]["show_images"].as_bool()), (Some("dark"), Some(false)));
    assert_eq!(got["settings"]["card_order"], "name");
    assert_eq!(call(&f.api, "PUT", "/api/settings", Some(json!({"theme": "purple"}))).0, 400);

    // The collection lists cards in the chosen order: by set, newest first, puts Core
    // Set 2019 (2018-07) before Dominaria (2018-04).
    call(&f.api, "PUT", "/api/settings", Some(json!({"card_order": "newest"})));
    assert_eq!(get(&f.api, "/api/games/mtg/cards")["cards"][0]["set"], "M19");
}

/// A photo reader that "reads" the photo's bytes as the text on the card.
fn fake_reader(data: &[u8]) -> anyhow::Result<Reading> {
    let text = String::from_utf8(data.to_vec())?;
    let lines =
        text.lines().enumerate().map(|(i, t)| Line { text: t.into(), y: 0.1 + 0.2 * i as f64, h: 0.05, ..Default::default() }).collect();
    Ok(Reading { lines, card_found: false })
}

#[test]
fn scanning() {
    let f = fixture();
    assert_eq!(call(&f.api, "POST", "/api/games/mtg/scan", None).0, 501);
    let api =
        Api::new("TCG Binder", "test", Client::Phone, Arc::new(SettingsStore::memory()), vec![f.svc.clone()]).with_reader(fake_reader);
    assert_eq!(get(&api, "/api/info")["games"][0]["Scan"], true);

    let resp = api.handle(&request("POST", "/api/games/mtg/scan/frame", b"Instant".to_vec()));
    let frame: Value = serde_json::from_slice(resp.body()).unwrap();
    assert_eq!((frame["key"].as_str(), frame["complete"].as_bool()), (Some("instant||"), Some(true)));

    // A name only: every printing to choose from, via the fake's fuzzy-less search.
    let resp = api.handle(&request("POST", "/api/games/mtg/scan", b"Lightning Bolt".to_vec()));
    let res: Value = serde_json::from_slice(resp.body()).unwrap();
    assert_eq!(resp.status(), 200, "{res}");
    assert_eq!(res["read"], "Lightning Bolt");
    assert!(res["candidates"].as_array().unwrap().is_empty()); // the fake has no /cards/named
    assert!(res["note"].as_str().unwrap().contains("Lightning Bolt"));
}
