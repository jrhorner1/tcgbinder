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
