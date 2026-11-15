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
fn sorting_filters_and_totals() {
    let f = fixture();
    let names = |path: &str| -> Vec<String> {
        get(&f.api, path)["cards"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap().to_string()).collect()
    };
    let all = get(&f.api, "/api/games/mtg/cards");
    assert_eq!((all["marketValue"].as_f64(), all["pricePaid"].as_f64()), (Some(0.40), Some(0.50)));
    assert_eq!((all["sort"].as_str(), all["dir"].as_str()), (Some("name"), Some("asc")), "the Card order setting");
    assert_eq!(names("/api/games/mtg/cards?sort=value&dir=desc"), ["Shock", "Opt"]);
    assert_eq!(names("/api/games/mtg/cards?sort=value"), ["Opt", "Shock"], "asc without dir");
    assert_eq!(names("/api/games/mtg/cards?sort=acquired&dir=desc"), ["Shock", "Opt"], "Opt is undated: last");
    assert_eq!(names("/api/games/mtg/cards?sort=released&dir=desc"), ["Shock", "Opt"]);

    call(&f.api, "PUT", "/api/settings", Some(json!({"card_order": "newest"})));
    let newest = get(&f.api, "/api/games/mtg/cards");
    assert_eq!((newest["sort"].as_str(), newest["dir"].as_str()), (Some("set"), Some("desc")));
    assert_eq!(names("/api/games/mtg/cards?sort=name"), ["Opt", "Shock"], "sort overrides the setting");

    assert_eq!(names("/api/games/mtg/cards?condition=LP"), ["Opt"]);
    assert_eq!(names("/api/games/mtg/cards?language=en&card_type=instant&sort=name"), ["Opt", "Shock"]);
    assert_eq!(names("/api/games/mtg/cards?price_min=0.1"), ["Shock"]);
    assert_eq!(names("/api/games/mtg/cards?acquired_from=2018-01-01&acquired_to=2018-12-31"), ["Shock"]);
    assert_eq!(names("/api/games/mtg/cards?min_copies=2"), ["Shock"]);
    assert_eq!(names("/api/games/mtg/cards?no_price_paid=1"), ["Opt"]);
    assert_eq!(names("/api/games/mtg/cards?unpriced=true"), ["Opt"]);
    assert_eq!(names("/api/games/mtg/cards?has_note=1").len(), 0);
    assert_eq!(names(&format!("/api/games/mtg/cards?{}=R", enc("f.Color identity"))).len(), 0, "no identity stored");
    let matched = get(&f.api, "/api/games/mtg/cards?min_copies=2");
    assert_eq!((matched["marketValue"].as_f64(), matched["pricePaid"].as_f64()), (Some(0.40), Some(0.50)));

    for bad in ["sort=nope", "sort=name&dir=up", "price_min=cheap", "price_max=-1", "acquired_from=last%20year"] {
        let (status, v) = call(&f.api, "GET", &format!("/api/games/mtg/cards?{bad}"), None);
        assert_eq!(status, 400, "{bad}: {v}");
    }
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

/// Downloads an export, returning its file name and body.
fn download(api: &Api, path: &str) -> (String, String) {
    let resp = api.handle(&request("GET", path, Vec::new()));
    assert_eq!(resp.status(), 200, "{path}");
    let disposition = resp.headers()[header::CONTENT_DISPOSITION].to_str().unwrap().to_string();
    (disposition, String::from_utf8(resp.body().clone()).unwrap())
}

#[test]
fn csv_export_reads_back() {
    let f = fixture();
    let mut shock = f.svc.store.get(SHOCK).unwrap();
    shock.add_purchase("foil", Condition::LightPlay, Purchase { quantity: 1, note: "trade, \"binder\"".into(), ..Default::default() });
    f.svc.store.save_card(SHOCK, shock).unwrap();

    let (name, csv) = download(&f.api, "/api/games/mtg/export?format=csv");
    assert!(name.contains(".csv"), "{name}");
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines[0], format!("{},Scryfall ID", tcgbinder_core::collection::CSV_COLUMNS.join(",")));
    assert_eq!(lines.len(), 4, "a row per purchase: {csv}");
    assert!(lines.contains(&format!("Shock,M19,Core Set 2019,156,en,nonfoil,NM,2,0.25,2018-08-08,,common,0.2,{SHOCK}").as_str()), "{csv}");

    // The list's filters and sort apply.
    let (_, red) = download(&f.api, "/api/games/mtg/export?format=csv&f.Color=R&sort=name");
    assert!(red.lines().skip(1).all(|l| l.starts_with("Shock,")), "{red}");
    let (_, blue) = download(&f.api, "/api/games/mtg/export?format=csv&f.Color=U");
    assert_eq!(blue.lines().count(), 2);

    // Imported into an empty collection, the same purchases come back.
    let (scryfall, _fake) = testing::search_server();
    let g = Magic::with_clients(scryfall, mtgjson::Client::new(&_fake.url));
    let dir = tempfile::tempdir().unwrap();
    let empty = GameService::from_store(Box::new(g), Store::open(dir.path().join("collection.json"), ID).unwrap()).unwrap();
    let res = empty.import(csv.as_bytes(), &mut |_, _| {}).unwrap();
    assert_eq!(res.report.format, "TCG Binder CSV");
    let rows = |cards: Vec<Card>| {
        let mut out: Vec<String> = cards
            .iter()
            .flat_map(|c| {
                c.purchase_rows().into_iter().map(move |r| {
                    let p = r.purchase;
                    format!(
                        "{}|{}|{}|{}|{}|{:?}|{}|{}|{}|{}",
                        c.name,
                        c.set.to_uppercase(),
                        c.collector_number,
                        c.language,
                        r.finish,
                        r.condition,
                        p.quantity,
                        p.price_paid,
                        p.acquired_date,
                        p.note
                    )
                })
            })
            .collect();
        out.sort();
        out
    };
    assert_eq!(rows(empty.store.cards()), rows(f.svc.store.cards()));
}

#[test]
fn valuation_report() {
    let f = fixture();
    let mut odd = f.svc.store.get(OPT).unwrap();
    odd.id = "zzzzzzzz-0000-4000-8000-000000000009".into();
    odd.name = "<script>alert('x')</script>".into();
    odd.set = "XYZ".into();
    odd.set_name = "<b>Set</b>".into();
    odd.released_at = String::new();
    f.svc.store.add_cards(vec![odd]).unwrap();

    let (name, html) = download(&f.api, "/api/games/mtg/export?format=report");
    assert!(name.contains("mtg-valuation-") && name.contains(".html"), "{name}");
    assert!(html.starts_with("<!DOCTYPE html>"));
    assert!(!html.contains("<script>") && !html.contains("<b>Set"), "card text is escaped");
    assert!(html.contains("&lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt;"));
    assert!(html.contains("Market value<b>$0.40</b>") && html.contains("Price paid<b>$0.50</b>"), "{html}");
    // Newest set first; per set a table with a total.
    let (m19, dom) = (html.find("Core Set 2019").unwrap(), html.find("Dominaria").unwrap());
    assert!(m19 < dom && dom < html.find("&lt;b&gt;Set&lt;/b&gt;").unwrap());
    assert!(html.contains("<td>Shock</td><td>156</td><td>Normal</td><td>Near Mint</td><td class=\"n\">2</td><td class=\"n\">$0.20</td><td class=\"n\">$0.40</td>"));
    assert!(html.contains("M19 total"));
    assert!(!html.contains("only the cards that matched"));

    let (_, red) = download(&f.api, "/api/games/mtg/export?format=report&f.Color=R");
    assert!(red.contains("only the cards that matched") && !red.contains("Dominaria"));

    assert_eq!(call(&f.api, "GET", "/api/games/mtg/export?format=pdf", None).0, 400);
    assert_eq!(call(&f.api, "GET", "/api/games/mtg/export?format=csv&sort=nope", None).0, 400);

    // Apps that save exports save these too.
    let out = tempfile::tempdir().unwrap();
    let api = Api::new("TCG Binder", "test", Client::Phone, Arc::new(SettingsStore::memory()), vec![f.svc.clone()])
        .with_export_dir(out.path().to_path_buf());
    let (status, saved) = call(&api, "POST", "/api/games/mtg/export?format=csv&set=m19", None);
    assert_eq!(status, 200, "{saved}");
    let path = saved["path"].as_str().unwrap();
    assert!(path.ends_with(".csv"));
    assert!(std::fs::read_to_string(path).unwrap().contains("Shock,M19"));
}

#[test]
fn sell_and_earnings() {
    let f = fixture();
    let sell = |key: &str, body: Value| call(&f.api, "POST", &format!("/api/games/mtg/card/sell?key={key}"), Some(body));
    let year = &tcgbinder_core::dates::today_string()[..4];
    // Shock: 2 non-foil NM bought at $0.25 on 2018-08-08.
    let shock = json!({"finish": "nonfoil", "condition": "NM", "price_paid": 0.25, "acquired_date": "2018-08-08", "note": ""});
    let with = |extra: Value| {
        let mut b = shock.clone();
        b.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        b
    };
    let (status, v) = sell(SHOCK, with(json!({"quantity": 3, "sale_each": 1.0})));
    assert_eq!(status, 400, "more than the purchase holds: {v}");
    let (status, v) = sell(SHOCK, with(json!({"price_paid": 0.3, "sale_each": 1.0})));
    assert_eq!(status, 400, "no such purchase: {v}");
    let (status, v) = sell(SHOCK, with(json!({"sale_each": -1.0})));
    assert_eq!(status, 400, "{v}");

    let (status, v) =
        sell(SHOCK, with(json!({"quantity": 1, "sale_each": 1.5, "fees": 0.2, "date": format!("{year}-01-15"), "sale_note": "eBay"})));
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["gain"].as_f64(), Some(1.05), "1.50 - 0.20 fees - 0.25 cost");
    assert_eq!(v["sale"]["card"]["name"], "Shock");
    assert_eq!(f.svc.store.get(SHOCK).unwrap().copy_count(), 1);
    let sold_id = v["sale"]["id"].as_str().unwrap().to_string();

    // Opt has no price paid: its sale's gain is unknown. Selling its only copy
    // deletes the card; the sale still reads.
    let (status, v) = sell(OPT, json!({"finish": "foil", "condition": "LP", "quantity": 1, "sale_each": 4.0, "date": "2025-06-01"}));
    assert_eq!(status, 200, "{v}");
    assert!(v["gain"].is_null());
    assert!(f.svc.store.get(OPT).is_none());
    let opt_id = v["sale"]["id"].as_str().unwrap().to_string();

    let all = get(&f.api, "/api/games/mtg/sales");
    assert_eq!(all["sales"][0]["card"]["name"], "Shock", "newest first");
    assert_eq!(all["years"], json!([year, "2025"]));
    let t = &all["totals"];
    assert_eq!((t["proceeds"].as_f64(), t["fees"].as_f64(), t["cost"].as_f64()), (Some(5.5), Some(0.2), Some(0.25)));
    assert_eq!((t["gain"].as_f64(), t["unknownCostCopies"].as_i64()), (Some(1.05), Some(1)));
    assert_eq!(all["months"].as_array().unwrap().len(), 2);
    let this_year = get(&f.api, &format!("/api/games/mtg/sales?year={year}"));
    assert_eq!(this_year["sales"].as_array().unwrap().len(), 1);
    assert_eq!(this_year["months"][0]["month"], format!("{year}-01"));
    assert_eq!(call(&f.api, "GET", "/api/games/mtg/sales?year=last", None).0, 400);
    assert_eq!(get(&f.api, "/api/games/mtg/home")["realisedThisYear"].as_f64(), Some(1.05));

    // Deleting with restore puts the copies back as they were, even for a card that left.
    let (status, _) = call(&f.api, "DELETE", &format!("/api/games/mtg/sales?id={opt_id}&restore=1"), None);
    assert_eq!(status, 200);
    let opt = f.svc.store.get(OPT).expect("Opt is back");
    assert_eq!((opt.name.as_str(), opt.set.as_str(), opt.copy_count()), ("Opt", "DOM", 1));
    assert_eq!(opt.purchases("foil", Condition::LightPlay).len(), 1);
    let (status, _) = call(&f.api, "DELETE", &format!("/api/games/mtg/sales?id={sold_id}&restore=1"), None);
    assert_eq!(status, 200);
    let shock = f.svc.store.get(SHOCK).unwrap();
    assert_eq!(shock.purchases("nonfoil", Condition::NearMint)[0].quantity, 2, "the purchase rejoins its copies");
    assert!(get(&f.api, "/api/games/mtg/sales")["sales"].as_array().unwrap().is_empty());

    // Without restore, the copies stay sold.
    let (_, v) = sell(SHOCK, with(json!({"quantity": 2, "sale_each": 1.0})));
    let id = v["sale"]["id"].as_str().unwrap().to_string();
    assert!(f.svc.store.get(SHOCK).is_none());
    call(&f.api, "DELETE", &format!("/api/games/mtg/sales?id={id}"), None);
    assert!(f.svc.store.get(SHOCK).is_none());
    assert_eq!(call(&f.api, "DELETE", "/api/games/mtg/sales?id=nope", None).0, 400);
}

#[test]
fn lists_and_decks() {
    let f = fixture();
    let (status, v) = call(&f.api, "POST", "/api/games/mtg/lists", Some(json!({"name": " "})));
    assert_eq!(status, 400, "{v}");
    let (status, deck) = call(&f.api, "POST", "/api/games/mtg/lists", Some(json!({"name": "Burn", "notes": "Modern"})));
    assert_eq!(status, 200, "{deck}");
    let id = deck["id"].as_str().unwrap().to_string();
    let path = |p: &str| format!("/api/games/mtg/lists{p}?id={id}");

    // Add the owned Shock (2 non-foil held) as 4 wanted, from its catalog card.
    let shock = get(&f.api, &format!("/api/games/mtg/card?key={SHOCK}"))["card"].clone();
    let (status, t) = call(&f.api, "POST", &path("/entries"), Some(json!({"card": shock, "quantity": 4})));
    assert_eq!(status, 200, "{t}");
    assert_eq!((t["cards"].as_i64(), t["owned"].as_i64(), t["missing"].as_i64()), (Some(4), Some(2), Some(2)));
    assert_eq!((t["marketValue"].as_f64(), t["missingCost"].as_f64()), (Some(0.8), Some(0.4)), "4 and 2 at $0.20");
    assert_eq!(call(&f.api, "POST", &path("/entries"), Some(json!({"card": shock, "quantity": 0}))).0, 400);

    // A pasted list: an exact line and a name-only line (any printing).
    let resp = f.api.handle(&request("POST", &path("/import"), b"2 Lightning Bolt (LEA) 1\n1 Opt\n1 Nonexistent Card\n".to_vec()));
    let v: Value = serde_json::from_slice(resp.body()).unwrap();
    assert_eq!(resp.status(), 200, "{v}");
    assert_eq!(v["added"], 3);
    assert_eq!(v["unmatched"], json!(["1 Nonexistent Card"]));

    let one = get(&f.api, &format!("/api/games/mtg/lists?id={id}"));
    let entries = one["entries"].as_array().unwrap();
    let find = |name: &str| entries.iter().find(|e| e["card"]["name"] == name).unwrap().clone();
    assert_eq!((find("Shock")["owned"].as_i64(), find("Shock")["any_printing"].as_bool()), (Some(2), Some(false)));
    assert_eq!(find("Lightning Bolt")["any_printing"], false);
    // Opt matched by name as the M19 printing; the DOM Opt held counts, since any printing does.
    assert_eq!((find("Opt")["any_printing"].as_bool(), find("Opt")["owned"].as_i64()), (Some(true), Some(1)));
    assert_eq!(one["list"]["notes"], "Modern");

    let resp = f.api.handle(&request("GET", &path("/export"), Vec::new()));
    let text = String::from_utf8(resp.body().clone()).unwrap();
    assert!(resp.headers()[header::CONTENT_DISPOSITION].to_str().unwrap().contains("Burn.txt"));
    assert!(text.contains("4 Shock (M19) 156\n") && text.contains("2 Lightning Bolt (LEA) 1\n") && text.contains("1 Opt\n"), "{text}");

    // Replacing the entries and renaming.
    // (Bolt's prices backdated, to see the refresh below reach them.)
    let keep: Vec<Value> = entries
        .iter()
        .filter(|e| e["card"]["name"] != "Opt")
        .map(|e| {
            let mut card = e["card"].clone();
            card["prices_updated"] = json!("2000-01-01");
            json!({"card": card, "quantity": e["quantity"], "any_printing": e["any_printing"], "finish": e["finish"]})
        })
        .collect();
    let (status, t) = call(&f.api, "PUT", &path(""), Some(json!({"name": "Burn v2", "entries": keep})));
    assert_eq!(status, 200, "{t}");
    assert_eq!((t["name"].as_str(), t["entries"].as_i64()), (Some("Burn v2"), Some(2)));
    let all = get(&f.api, "/api/games/mtg/lists");
    assert_eq!(all[0]["name"], "Burn v2");

    // Price refreshes reach list cards too.
    f.svc.refresh_prices(&mut |_, _| {}).unwrap();
    let bolt = f.svc.lists.get(&id).unwrap().entries.into_iter().find(|e| e.card.name == "Lightning Bolt").unwrap();
    assert_eq!(bolt.card.prices_updated, tcgbinder_core::dates::today_string(), "refreshed with the collection");

    assert_eq!(call(&f.api, "DELETE", &path(""), None).0, 200);
    assert_eq!(call(&f.api, "GET", &path(""), None).0, 404);
}

#[test]
fn watchlist_and_alerts() {
    let f = fixture();
    let heard = Arc::new(std::sync::Mutex::new(Vec::new()));
    let h = heard.clone();
    f.svc.set_alert_hook(Box::new(move |game, w| h.lock().unwrap().push(format!("{game} {} {:?}", w.card.name, w.price()))));
    let shock = get(&f.api, &format!("/api/games/mtg/card?key={SHOCK}"))["card"].clone();
    let watch = |body: Value| call(&f.api, "POST", "/api/games/mtg/watchlist", Some(body));

    let (status, w) = watch(json!({"card": shock, "finish": "nonfoil", "rule": {"kind": "percent", "value": 20}}));
    assert_eq!(status, 200, "{w}");
    assert_eq!((w["baseline"].as_f64(), w["key"].as_str()), (Some(0.2), Some(format!("{SHOCK}/nonfoil").as_str())));
    assert_eq!(watch(json!({"card": shock, "finish": "nonfoil"})).0, 400, "already watched");
    assert_eq!(watch(json!({"card": shock, "finish": "etched"})).0, 400, "no price in that finish for a percent rule");
    assert_eq!(watch(json!({"card": shock, "finish": "foil", "rule": {"kind": "above", "value": 0}})).0, 400);
    let (status, _) = watch(json!({"card": shock, "finish": "foil", "rule": {"kind": "below", "value": 0.5}}));
    assert_eq!(status, 200);

    // The fake catalog prices every card at $0.25, non-foil only: non-foil Shock moves
    // 25% (an alert); foil Shock now has no price, so it can't cross.
    f.svc.refresh_prices(&mut |_, _| {}).unwrap();
    assert_eq!(*heard.lock().unwrap(), ["mtg Shock Some(0.25)"]);
    let home = get(&f.api, "/api/games/mtg/home");
    assert_eq!(home["alerts"].as_array().unwrap().len(), 1);
    let list = get(&f.api, "/api/games/mtg/watchlist");
    let nonfoil = list.as_array().unwrap().iter().find(|w| w["finish"] == "nonfoil").unwrap().clone();
    assert_eq!((nonfoil["change"].as_f64(), nonfoil["changePercent"].as_f64()), (Some(0.05), Some(25.0)));
    assert_eq!(nonfoil["triggered"]["price"].as_f64(), Some(0.25));

    let key = enc(nonfoil["key"].as_str().unwrap());
    let path = format!("/api/games/mtg/watchlist?key={key}");
    let (status, w) = call(&f.api, "PUT", &path, Some(json!({"action": "dismiss"})));
    assert_eq!((status, w["triggered"].is_null(), w["baseline"].as_f64()), (200, true, Some(0.2)));
    let (_, w) = call(&f.api, "PUT", &path, Some(json!({"action": "reset"})));
    assert_eq!((w["baseline"].as_f64(), w["change"].as_f64()), (Some(0.25), Some(0.0)));
    let (_, w) = call(&f.api, "PUT", &path, Some(json!({"rule": {"kind": "above", "value": 2}})));
    assert_eq!(w["rule"], json!({"kind": "above", "value": 2.0}));
    assert_eq!(call(&f.api, "PUT", &path, Some(json!({"action": "snooze"}))).0, 400);
    assert!(get(&f.api, "/api/games/mtg/home")["alerts"].as_array().unwrap().is_empty());
    assert_eq!(call(&f.api, "DELETE", &path, None).0, 200);
    assert_eq!(call(&f.api, "DELETE", &path, None).0, 404);
    assert_eq!(get(&f.api, "/api/games/mtg/watchlist").as_array().unwrap().len(), 1);
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

#[test]
fn status_notices_another_devices_change() {
    let f = fixture();
    let before = get(&f.api, "/api/status")["revision"].as_u64().unwrap();
    assert_eq!(get(&f.api, "/api/status")["revision"].as_u64().unwrap(), before, "nothing changed");
    // Another device, sharing the folder, removes a card.
    let other = Store::open(f.svc.store.file_path(), ID).unwrap();
    other.remove_card(OPT).unwrap();
    assert!(get(&f.api, "/api/status")["revision"].as_u64().unwrap() > before);
    assert!(f.svc.store.get(OPT).is_none());
}

/// An export with EchoMTG's real header and made-up rows: four copies the fake
/// Scryfall knows, a token and a basic land it doesn't.
const ECHOMTG: &[u8] = include_bytes!("../../mtg/tests/echomtg-export.txt");

fn preview(f: &Fixture) -> Value {
    let resp = f.api.handle(&request("POST", "/api/games/mtg/import?preview=1", ECHOMTG.to_vec()));
    assert_eq!(resp.status(), 200);
    serde_json::from_slice(resp.body()).unwrap()
}

fn commit(f: &Fixture, token: &Value) -> (u16, Value) {
    call(&f.api, "POST", "/api/games/mtg/import/commit", Some(json!({ "token": token })))
}

#[test]
fn import_preview_then_commit() {
    let direct = fixture();
    let resp = direct.api.handle(&request("POST", "/api/games/mtg/import", ECHOMTG.to_vec()));
    assert_eq!(resp.status(), 200);
    let imported: Value = serde_json::from_slice(resp.body()).unwrap();

    let f = fixture();
    let (revision, cards) = (f.svc.store.revision(), f.svc.store.len());
    let p = preview(&f);
    assert_eq!(
        (p["format"].as_str(), p["rows"].as_u64(), p["copies"].as_i64(), p["cards"].as_u64()),
        (Some("EchoMTG"), Some(6), Some(4), Some(3))
    );
    assert_eq!((p["newCards"].as_u64(), p["alreadyOwned"].as_u64(), p["pricePaid"].as_f64()), (Some(2), Some(2), Some(1.95)));
    assert_eq!(p["header"].as_str(), std::str::from_utf8(ECHOMTG).unwrap().lines().next());
    let lines: Vec<u64> = p["unmatched"].as_array().unwrap().iter().map(|u| u["line"].as_u64().unwrap()).collect();
    assert_eq!(lines, [6, 7]);
    assert!(p["unmatched"][0]["text"].as_str().unwrap().contains("Soldier Token"));
    assert_eq!((f.svc.store.revision(), f.svc.store.len()), (revision, cards), "a preview saves nothing");

    assert_eq!(commit(&f, &json!("not the token")).0, 410);
    let (status, mut committed) = commit(&f, &p["token"]);
    assert_eq!(status, 200);
    let mut imported = imported;
    for v in [&mut committed, &mut imported] {
        let undo = v.as_object_mut().unwrap().remove("undo").unwrap();
        assert!(undo["backup"].as_str().unwrap().ends_with("-import.json"));
    }
    assert_eq!(committed, imported);
    let all = |f: &Fixture| serde_json::to_value(f.svc.store.cards()).unwrap();
    assert_eq!(all(&f), all(&direct));
    assert_eq!(commit(&f, &p["token"]).0, 410, "a preview is committed once");
}

#[test]
fn a_new_preview_replaces_the_old_one() {
    let f = fixture();
    let first = preview(&f);
    let second = preview(&f);
    assert_eq!(commit(&f, &first["token"]).0, 410);
    assert_eq!(commit(&f, &second["token"]).0, 200);
}

#[test]
fn undo_a_bulk_delete_and_restore_backups() {
    let f = fixture();
    let (status, r) = call(&f.api, "POST", "/api/games/mtg/bulk", Some(json!({ "keys": [SHOCK, OPT], "delete": true })));
    assert_eq!((status, r["Deleted"].as_u64()), (200, Some(2)));
    assert!(f.svc.store.is_empty());
    let undo = r["undo"].clone();
    assert!(undo["backup"].as_str().unwrap().ends_with("-bulk-delete.json"));

    let list = get(&f.api, "/api/games/mtg/backups");
    assert_eq!(
        (list[0]["name"].clone(), list[0]["reason"].as_str(), list[0]["cards"].as_u64()),
        (undo["backup"].clone(), Some("bulk-delete"), Some(2))
    );

    // Undo only while nothing else has changed.
    let stale = json!({ "name": undo["backup"], "ifRevision": undo["revision"].as_u64().unwrap() + 1 });
    assert_eq!(call(&f.api, "POST", "/api/games/mtg/backups/restore", Some(stale)).0, 409);
    let (status, restored) =
        call(&f.api, "POST", "/api/games/mtg/backups/restore", Some(json!({ "name": undo["backup"], "ifRevision": undo["revision"] })));
    assert_eq!((status, restored["cards"].as_u64()), (200, Some(2)));
    assert!(f.svc.store.get(SHOCK).is_some());
    assert_eq!(get(&f.api, "/api/games/mtg/backups")[0]["reason"], "restore", "a restore backs up first");

    for name in ["../collection.json", "collection.json", "x/../../collection.json"] {
        assert_eq!(call(&f.api, "POST", "/api/games/mtg/backups/restore", Some(json!({ "name": name }))).0, 404, "{name}");
    }
}

/// An app whose local folder is also its data folder (the default), holding one card,
/// and a second folder `target` to move it to.
struct FolderFixture {
    api: Api,
    svc: Arc<GameService>,
    settings: Arc<SettingsStore>,
    local: std::path::PathBuf,
    target: std::path::PathBuf,
    _dir: TempDir,
    _scryfall: FakeServer,
}

fn folder_fixture(fixed: bool) -> FolderFixture {
    let (scryfall, fake) = testing::search_server();
    let dir = tempfile::tempdir().unwrap();
    let (local, target) = (dir.path().join("local"), dir.path().join("Cloud Drive/Cards"));
    let g = Magic::with_clients(scryfall, mtgjson::Client::new(&fake.url));
    let svc = Arc::new(GameService::open(&local, &local, Box::new(g)).unwrap());
    let card = Card { id: SHOCK.into(), name: "Shock".into(), set: "M19".into(), language: "en".into(), ..Card::default() };
    svc.store.add_cards(vec![held(card, "nonfoil", Condition::NearMint, Purchase { quantity: 2, ..Default::default() })]).unwrap();
    let settings = Arc::new(SettingsStore::open(&local));
    let folders = Folders { local: local.clone(), data: local.clone(), fixed, first_run: false };
    let api = Api::new("TCG Binder", "test", Client::Web, settings.clone(), vec![svc.clone()]).with_folders(folders);
    FolderFixture { api, svc, settings, local, target, _dir: dir, _scryfall: fake }
}

#[test]
fn move_the_data_folder() {
    let f = folder_fixture(false);
    let info = get(&f.api, "/api/data-folder");
    assert_eq!((info["isDefault"].as_bool(), info["fixed"].as_bool(), info["canPick"].as_bool()), (Some(true), Some(false), Some(false)));
    assert_eq!(info["path"].as_str(), f.local.to_str());
    assert_eq!(call(&f.api, "POST", "/api/data-folder/pick", None).0, 501);

    let target = f.target.display().to_string();
    let (status, seen) = call(&f.api, "POST", "/api/data-folder/inspect", Some(json!({ "path": target })));
    assert_eq!(
        (status, seen["exists"].as_bool(), seen["writable"].as_bool(), seen["games"].as_array().map(Vec::len)),
        (200, Some(false), Some(true), Some(0))
    );
    assert_eq!(call(&f.api, "POST", "/api/data-folder/inspect", Some(json!({ "path": "relative" }))).0, 400);
    assert_eq!(call(&f.api, "POST", "/api/data-folder", Some(json!({ "path": target, "mode": "sideways" }))).0, 400);
    assert_eq!(call(&f.api, "PUT", "/api/settings", Some(json!({ "data_dir": target }))).0, 400, "not through settings");

    let (status, moved) = call(&f.api, "POST", "/api/data-folder", Some(json!({ "path": target, "mode": "move" })));
    assert_eq!((status, moved["restart"].as_bool(), moved["restartsItself"].as_bool()), (200, Some(true), Some(false)));
    assert_eq!(f.settings.get().data_dir, target);
    assert!(!f.local.join("mtg/collection.json").exists());
    let found = tcgbinder_core::datafolder::inspect(&f.target, &["mtg"]);
    assert_eq!(found.games[0].cards, 1);
    assert!(f.target.join("mtg/backups").read_dir().unwrap().any(|e| e.unwrap().file_name().to_string_lossy().ends_with("-move.json")));
    // Until the app restarts, nothing may be saved (the open collection has moved).
    assert_eq!(call(&f.api, "DELETE", &format!("/api/games/mtg/card?key={SHOCK}"), None).0, 409);
}

#[test]
fn a_fixed_data_folder_stays() {
    let f = folder_fixture(true);
    let (status, _) = call(&f.api, "POST", "/api/data-folder", Some(json!({ "path": f.target.display().to_string(), "mode": "move" })));
    assert_eq!(status, 409);
    assert!(f.local.join("mtg/collection.json").exists());
}

#[test]
fn merge_or_discard_conflict_copies() {
    let f = folder_fixture(false);
    let game_dir = f.local.join("mtg");
    // Another device saved at the same time: Shock ×3 and a card this one lacks.
    let theirs = json!({ "game": "mtg", "cards": {
        SHOCK: { "id": SHOCK, "name": "Shock", "lang": "en", "copies": { "nonfoil": { "NM": [{ "quantity": 3 }] } } },
        OPT: { "id": OPT, "name": "Opt", "lang": "en", "copies": { "foil": { "LP": [{ "quantity": 1 }] } } },
    }});
    std::fs::write(game_dir.join("collection 2.json"), theirs.to_string()).unwrap();
    std::fs::write(game_dir.join("collection (conflicted copy).json"), theirs.to_string()).unwrap();
    let conflicts = get(&f.api, "/api/data-folder")["conflicts"].clone();
    assert_eq!(conflicts.as_array().unwrap().len(), 2);
    assert_eq!((conflicts[1]["name"].as_str(), conflicts[1]["cards"].as_u64()), (Some("collection 2.json"), Some(2)));

    let (status, merged) = call(&f.api, "POST", "/api/games/mtg/conflicts/merge", Some(json!({ "name": "collection 2.json" })));
    assert_eq!((status, merged["changed"].as_u64()), (200, Some(2)));
    assert_eq!(f.svc.store.get(SHOCK).unwrap().copy_count(), 3, "the larger quantity, not 2 + 3");
    assert!(f.svc.store.get(OPT).is_some());
    assert!(!game_dir.join("collection 2.json").exists());
    assert!(merged["undo"]["backup"].as_str().unwrap().ends_with("-merge.json"));

    let (status, _) =
        call(&f.api, "POST", "/api/games/mtg/conflicts/discard", Some(json!({ "name": "collection (conflicted copy).json" })));
    assert_eq!(status, 200);
    assert!(get(&f.api, "/api/data-folder")["conflicts"].as_array().unwrap().is_empty());
    for name in ["collection.json", "../local/settings.json", "price-history.json"] {
        assert_eq!(call(&f.api, "POST", "/api/games/mtg/conflicts/discard", Some(json!({ "name": name }))).0, 404, "{name}");
    }
    assert!(game_dir.join("collection.json").exists());
}

#[test]
fn notes_on_purchases() {
    let f = fixture();
    let rows = json!([
        { "finish": "nonfoil", "condition": "NM", "quantity": 1, "price_paid": 0.25, "acquired_date": "2018-08-08", "note": "  Binder A  " },
        { "finish": "nonfoil", "condition": "NM", "quantity": 1, "price_paid": 0.25, "acquired_date": "2018-08-08", "note": "" },
    ]);
    let (status, _) = call(&f.api, "PUT", &format!("/api/games/mtg/card?key={SHOCK}"), Some(json!({ "purchases": rows })));
    assert_eq!(status, 200);
    let card = &get(&f.api, &format!("/api/games/mtg/card?key={SHOCK}"))["card"];
    let notes: Vec<&str> = card["purchases"].as_array().unwrap().iter().map(|p| p["note"].as_str().unwrap()).collect();
    assert_eq!(notes.len(), 2, "different notes keep purchases apart");
    assert!(notes.contains(&"Binder A"));
    assert_eq!(get(&f.api, "/api/games/mtg/cards?q=binder")["cards"][0]["name"], "Shock");

    let (status, _) = call(&f.api, "POST", "/api/games/mtg/bulk", Some(json!({ "keys": [SHOCK], "note": "" })));
    assert_eq!(status, 200);
    let card = &get(&f.api, &format!("/api/games/mtg/card?key={SHOCK}"))["card"];
    assert_eq!(card["purchases"].as_array().unwrap().len(), 1, "cleared notes join up again");
    assert_eq!(get(&f.api, "/api/games/mtg/cards?q=binder")["cards"].as_array().unwrap().len(), 0);
}

#[test]
fn printings_in_a_language() {
    let f = fixture();
    let (status, _) = call(&f.api, "GET", &format!("/api/games/mtg/printings?key={SHOCK}&lang=ja"), None);
    assert_eq!(status, 200);
    let calls = f._scryfall.calls.lock().unwrap().clone();
    assert!(calls.iter().any(|c| c.contains("lang%3Aja")), "{calls:?}");
    let langs = get(&f.api, "/api/games/mtg/filters")["languages"].clone();
    assert_eq!((langs[0]["code"].as_str(), langs[0]["name"].as_str()), (Some("en"), Some("English")));
}

#[test]
fn paste_a_list() {
    let f = fixture();
    let list = "Deck\n4 Lightning Bolt (M11) 1\n2x Opt [DOM]\n1 Shock (M19) 156 *F*\n\nSideboard\n1 Not a Card (ZZZ) 9\n";
    let resp = f.api.handle(&request("POST", "/api/games/mtg/import?preview=1", list.as_bytes().to_vec()));
    assert_eq!(resp.status(), 200);
    let p: Value = serde_json::from_slice(resp.body()).unwrap();
    // Opt has no number, so it's matched by name (the fake's M19 printing).
    assert_eq!((p["format"].as_str(), p["rows"].as_u64(), p["copies"].as_i64()), (Some("Card list"), Some(4), Some(7)));
    let lines: Vec<u64> = p["unmatched"].as_array().unwrap().iter().map(|u| u["line"].as_u64().unwrap()).collect();
    assert_eq!(lines, [7]);
    assert_eq!(p["unmatched"][0]["text"], "1 Not a Card (ZZZ) 9");
    let (status, done) = commit(&f, &p["token"]);
    assert_eq!((status, done["cards"].as_i64()), (200, Some(7)));
    assert_eq!(f.svc.store.get("aaaaaaaa-0000-4000-8000-000000000001").unwrap().finish_copy_count("foil"), 1);
}

#[test]
fn autocomplete_answers() {
    let f = fixture();
    let (status, r) = call(&f.api, "GET", "/api/games/mtg/autocomplete?q=s", None);
    assert_eq!((status, r["names"].as_array().map(Vec::len)), (200, Some(0)));
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

#[test]
fn a_lot_counted_as_entered() {
    let f = fixture();
    let (status, lot) = call(
        &f.api,
        "POST",
        "/api/games/mtg/lots",
        Some(
            json!({ "name": "eBay lot", "total_paid": 9.0, "acquired_date": "2026-11-01", "count": { "kind": "as_entered" }, "split": "even" }),
        ),
    );
    assert_eq!((status, lot["open"].as_bool()), (200, Some(true)));
    let id = lot["id"].clone();
    let r = add_in_lot(&f, "lot-a", 1.0, 1, &id);
    assert_eq!(r["lot"]["priceEach"].as_f64(), Some(9.0));
    assert_eq!(lot_purchases(&f, "lot-a", &id), [(9.0, "2026-11-01".into(), 1)], "the lot sets price and date");
    let r = add_in_lot(&f, "lot-b", 1.0, 2, &id);
    assert_eq!((r["lot"]["entered"].as_i64(), r["lot"]["priceEach"].as_f64()), (Some(3), Some(3.0)));
    assert_eq!(lot_purchases(&f, "lot-a", &id)[0].0, 3.0, "earlier cards are repriced");

    // Editing the total reprices.
    let (status, _) = call(&f.api, "PUT", &format!("/api/games/mtg/lots?id={}", id.as_str().unwrap()), Some(json!({ "total_paid": 6.0 })));
    assert_eq!(status, 200);
    assert_eq!(lot_purchases(&f, "lot-b", &id)[0].0, 2.0);
    // The card editor keeps a copy's lot.
    let c = get(&f.api, "/api/games/mtg/card?key=lot-b")["card"].clone();
    let (status, _) = call(&f.api, "PUT", "/api/games/mtg/card?key=lot-b", Some(json!({ "purchases": c["purchases"] })));
    assert_eq!(status, 200);
    assert_eq!(lot_purchases(&f, "lot-b", &id).len(), 1);

    // Keep: the copies stay with their prices, outside any lot.
    let (status, _) = call(&f.api, "DELETE", &format!("/api/games/mtg/lots?id={}&cards=keep", id.as_str().unwrap()), None);
    assert_eq!(status, 200);
    let b = &get(&f.api, "/api/games/mtg/card?key=lot-b")["card"]["purchases"][0];
    assert_eq!((b["lot"].as_str(), b["price_paid"].as_f64()), (Some(""), Some(2.0)));
    assert_eq!(get(&f.api, "/api/games/mtg/lots").as_array().unwrap().len(), 0);
}

#[test]
fn finishing_a_value_lot() {
    let f = fixture();
    let (_, lot) = call(
        &f.api,
        "POST",
        "/api/games/mtg/lots",
        Some(json!({ "name": "Bundle", "total_paid": 55.0, "acquired_date": "2026-11-02", "count": { "kind": "known", "cards": 5 } })),
    );
    let id = lot["id"].clone();
    let path = format!("/api/games/mtg/lots?id={}", id.as_str().unwrap());
    add_in_lot(&f, "v-a", 30.0, 1, &id);
    add_in_lot(&f, "v-b", 50.0, 1, &id);
    assert_eq!(lot_purchases(&f, "v-a", &id)[0].0, 11.0, "open: $55 / 5 counted cards");

    let (status, r) = call(&f.api, "PUT", &path, Some(json!({ "open": false })));
    assert_eq!(status, 200, "{r}");
    assert_eq!((r["short"]["entered"].as_i64(), r["short"]["expected"].as_i64()), (Some(2), Some(5)));
    assert_eq!((r["outcome"]["marketValue"].as_f64(), r["outcome"]["percent"].as_f64()), (Some(80.0), Some(68.8)));
    assert_eq!(lot_purchases(&f, "v-a", &id)[0].0, 20.625);
    assert_eq!(lot_purchases(&f, "v-b", &id)[0].0, 34.375);
    assert!(!r["lot"]["split_date"].as_str().unwrap().is_empty());
    // A finished lot takes no more cards until it's continued.
    let body = json!({ "card": catalog("v-c", 1.0), "quantity": 1, "lot": id });
    assert_eq!(call(&f.api, "POST", "/api/games/mtg/cards", Some(body)).0, 400);

    // Remove: the copies go, after a backup.
    let (status, r) = call(&f.api, "DELETE", &format!("{path}&cards=remove"), None);
    assert_eq!(status, 200);
    assert!(r["undo"]["backup"].as_str().unwrap().ends_with("-bulk-delete.json"));
    assert!(f.svc.store.get("v-a").is_none());
}

#[test]
fn import_into_a_lot() {
    let f = fixture();
    let (_, lot) = call(
        &f.api,
        "POST",
        "/api/games/mtg/lots",
        Some(
            json!({ "name": "Paste", "total_paid": 8.0, "acquired_date": "2026-11-03", "count": { "kind": "as_entered" }, "split": "even" }),
        ),
    );
    let resp = f.api.handle(&request("POST", "/api/games/mtg/import?preview=1", b"4 Lightning Bolt (M11) 1\n".to_vec()));
    let p: Value = serde_json::from_slice(resp.body()).unwrap();
    let (status, r) = call(&f.api, "POST", "/api/games/mtg/import/commit", Some(json!({ "token": p["token"], "lot": lot["id"] })));
    assert_eq!((status, r["lot"]["entered"].as_i64(), r["lot"]["priceEach"].as_f64()), (200, Some(4), Some(2.0)));
    assert_eq!(lot_purchases(&f, "aaaaaaaa-0000-4000-8000-000000000101", &lot["id"]), [(2.0, "2026-11-03".into(), 4)]);
}

#[test]
fn jobs_report_progress() {
    let f = fixture();
    assert_eq!(get(&f.api, "/api/games/mtg/jobs")["jobs"].as_array().unwrap().len(), 0);
    let seen = std::cell::RefCell::new(Value::Null);
    let res = f.api.job(&f.svc, "import", |r| {
        r.report(3, 10, "Matching cards");
        let Reply::Json(v) = f.api.jobs(&f.svc) else { unreachable!() };
        *seen.borrow_mut() = v;
        // The same job can't start twice.
        assert!(f.api.job(&f.svc, "import", |_| Ok(Value::Null)).is_err());
        Ok(json!({}))
    });
    assert!(res.is_ok());
    let jobs = seen.into_inner();
    assert_eq!(jobs["jobs"][0], json!({ "name": "import", "done": 3, "total": 10, "message": "Matching cards" }));
    assert_eq!(get(&f.api, "/api/games/mtg/jobs")["jobs"].as_array().unwrap().len(), 0, "gone once finished");
}
