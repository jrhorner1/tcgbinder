//! Searching the catalog, autocomplete and scanning.

use super::*;

/// A photo reader that "reads" the photo's bytes as the text on the card.
fn fake_reader(data: &[u8]) -> anyhow::Result<Reading> {
    let text = String::from_utf8(data.to_vec())?;
    let lines =
        text.lines().enumerate().map(|(i, t)| Line { text: t.into(), y: 0.1 + 0.2 * i as f64, h: 0.05, ..Default::default() }).collect();
    Ok(Reading { lines, card_found: false })
}

#[test]
fn rules_text_is_searched() {
    let f = fixture();
    // The catalog: rules=1 finds cards by their rules text, one result per card.
    let found = get(&f.api, "/api/games/mtg/search?text=3%20damage&rules=1");
    assert_eq!(found["cards"].as_array().unwrap().len(), 1);
    assert_eq!(found["cards"][0]["name"], "Lightning Bolt");
    assert!(get(&f.api, "/api/games/mtg/search?text=draw%20a%20card&rules=1")["cards"].as_array().unwrap().is_empty());
    let calls = f._scryfall.calls.lock().unwrap().clone();
    assert!(calls.iter().any(|c| c.contains("unique=cards") && c.contains("order=name")), "{calls:?}");

    // The collection: the search at the top finds a card by its rules text; the
    // collection list's own filter doesn't.
    let card = get(&f.api, "/api/games/mtg/search?text=bolt")["cards"][0].clone();
    call(&f.api, "POST", "/api/games/mtg/cards", Some(json!({"card": card, "quantity": 1, "finish": "nonfoil", "condition": "NM"})));
    let all = get(&f.api, "/api/search?q=any%20target");
    assert_eq!(all["games"][0]["Total"], 1);
    assert_eq!(all["games"][0]["Cards"][0]["name"], "Lightning Bolt");
    assert_eq!(get(&f.api, "/api/games/mtg/cards?q=any%20target")["matched"], 0);

    // Cards named with the words come before cards that only mention them.
    let mut opt = f.svc.store.get(OPT).unwrap();
    opt.rules_text = "Scry 1. Draw a card. Then lightning strikes.".into();
    f.svc.store.save_card(OPT, opt).unwrap();
    let names: Vec<String> = get(&f.api, "/api/search?q=lightning")["games"][0]["Cards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(names, ["Lightning Bolt", "Opt"]);
}

#[test]
fn a_catalog_card_by_id() {
    let f = fixture();
    // A printing you don't own: the card, its details and its catalog link.
    let d = get(&f.api, "/api/games/mtg/catalog?id=aaaaaaaa-0000-4000-8000-000000000101");
    assert_eq!(d["card"]["name"], "Lightning Bolt");
    assert_eq!(d["card"]["key"], "aaaaaaaa-0000-4000-8000-000000000101");
    assert!(d["details"].is_array());
    assert!(d["others"].as_array().unwrap().is_empty());
    // One you do own is found the same way (its key is its catalog ID).
    assert_eq!(get(&f.api, &format!("/api/games/mtg/catalog?id={SHOCK}"))["card"]["name"], "Shock");
    // Not in the catalog, or not an ID at all: 404.
    assert_eq!(call(&f.api, "GET", "/api/games/mtg/catalog?id=aaaaaaaa-0000-4000-8000-000000000999", None).0, 404);
    assert_eq!(call(&f.api, "GET", "/api/games/mtg/catalog?id=shock", None).0, 404);
}

#[test]
fn the_catalog_sets() {
    let f = fixture();
    let cache = tempfile::tempdir().unwrap();
    f.svc.game.open(cache.path()).unwrap(); // the set list is cached there
    let sets = get(&f.api, "/api/games/mtg/catalog/sets");
    let first = &sets["sets"][0];
    assert_eq!((first["code"].as_str(), first["name"].as_str()), (Some("m19"), Some("Core Set 2019")));
    assert!(first["released"].is_string());
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

#[test]
fn autocomplete_answers() {
    let f = fixture();
    let (status, r) = call(&f.api, "GET", "/api/games/mtg/autocomplete?q=s", None);
    assert_eq!((status, r["names"].as_array().map(Vec::len)), (200, Some(0)));
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
