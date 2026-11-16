//! Lists and decks.

use super::*;

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
