//! Home, Statistics, a set's page and the change revision.

use super::*;

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
fn statistics_page() {
    let f = fixture();
    let s = get(&f.api, "/api/games/mtg/stats");
    let table = |id: &str| s["breakdowns"].as_array().unwrap().iter().find(|t| t["id"] == id).unwrap().clone();
    let ids: Vec<&str> = s["breakdowns"].as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["rarity", "finish", "condition", "language", "year", "Color", "Card type"]);
    // Shock: 2 non-foil NM ($0.20, paid $0.25, 2018-08-08); Opt: 1 foil LP, unpriced, undated.
    assert_eq!(table("rarity")["rows"][0], json!({"key": "common", "cards": 3, "unique": 2, "value": 0.4, "paid": 0.5}));
    assert_eq!(table("finish")["param"], "finish");
    assert_eq!(table("Color")["param"], "f.Color");
    let color = table("Color");
    let colors: Vec<&str> = color["rows"].as_array().unwrap().iter().map(|r| r["key"].as_str().unwrap()).collect();
    assert_eq!(colors, ["U", "R"], "the game's order");
    assert_eq!(table("Card type")["rows"][0]["key"], "Instant");
    assert!(table("year")["param"].is_null());
    assert_eq!(s["byMonth"], json!([{"month": "2018-08", "cards": 2, "paid": 0.5}]));
    assert_eq!(s["undated"], 1);
    let under = s["bands"].as_array().unwrap().iter().find(|b| b["label"] == "Under $1").unwrap().clone();
    assert_eq!((under["cards"].as_i64(), under["share"].as_f64(), under["max"].as_f64()), (Some(2), Some(1.0), Some(1.0)));
    assert_eq!(s["bands"].as_array().unwrap().last().unwrap()["label"], "No price");
    assert_eq!(s["mostCopies"][0]["name"], "Shock", "Opt has one copy, so isn't listed");
    assert_eq!(s["mostCopies"].as_array().unwrap().len(), 1);
}

#[test]
fn completion_of_the_printed_set() {
    let f = fixture();
    let cache = tempfile::tempdir().unwrap();
    f.svc.game.open(cache.path()).unwrap(); // the set list is cached there
    f.svc.sets().unwrap();
    // M19's fake cards: Lightning Bolt 1, Opt 60, Shock 156 (owned); printed size 100.
    let set = get(&f.api, "/api/games/mtg/set?code=m19");
    assert_eq!((set["collected"].as_i64(), set["size"].as_i64()), (Some(0), Some(100)), "Shock is outside the printed run");
    let m = get(&f.api, "/api/games/mtg/missing?set=m19");
    let names: Vec<&str> = m["missing"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Lightning Bolt", "Opt"]);
    assert_eq!(
        (m["total"].as_i64(), m["printedSize"].as_i64(), m["totalCost"].as_f64(), m["unpriced"].as_i64()),
        (Some(2), Some(100), Some(0.5), Some(0))
    );
    assert_eq!(get(&f.api, "/api/games/mtg/home")["bySet"][0]["Size"], 100);

    call(&f.api, "PUT", "/api/settings", Some(json!({"set_completion": "all"})));
    let set = get(&f.api, "/api/games/mtg/set?code=m19");
    assert_eq!((set["collected"].as_i64(), set["size"].as_i64()), (Some(1), Some(4)));
    let m = get(&f.api, "/api/games/mtg/missing?set=m19");
    assert_eq!((m["total"].as_i64(), m["printedSize"].is_null(), m["missing"].as_array().unwrap().len()), (Some(3), true, 2));
    assert_eq!(get(&f.api, "/api/settings")["choices"]["set_completion"], json!(["main", "all"]));
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
