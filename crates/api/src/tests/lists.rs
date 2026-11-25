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

#[test]
fn missing_lists_cross_off_cards_once_owned() {
    let f = fixture();
    let (status, list) = call(&f.api, "POST", "/api/games/mtg/lists", Some(json!({ "name": "TST Missing", "remove_owned": true })));
    assert_eq!((status, list["remove_owned"].as_bool()), (200, Some(true)), "{list}");
    let id = list["id"].as_str().unwrap().to_string();
    let entries = json!({ "entries": [{ "card": catalog("one", 1.0), "quantity": 1 }, { "card": catalog("two", 2.0), "quantity": 1 }] });
    assert_eq!(call(&f.api, "PUT", &format!("/api/games/mtg/lists?id={id}"), Some(entries)).0, 200);

    // Card "one" is added to the collection: opening the list takes it off.
    let (status, _) =
        call(&f.api, "POST", "/api/games/mtg/cards", Some(json!({ "card": catalog("one", 1.0), "quantity": 1, "finish": "nonfoil" })));
    assert_eq!(status, 200);
    let shown = get(&f.api, &format!("/api/games/mtg/lists?id={id}"));
    let ids: Vec<&str> = shown["entries"].as_array().unwrap().iter().map(|e| e["card"]["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["two"]);
    assert_eq!(get(&f.api, "/api/games/mtg/lists")[0]["entries"].as_i64(), Some(1));

    // Turned off, owned cards stay.
    let (status, v) = call(&f.api, "PUT", &format!("/api/games/mtg/lists?id={id}"), Some(json!({ "remove_owned": false })));
    assert_eq!((status, v["remove_owned"].as_bool()), (200, Some(false)));
    call(&f.api, "POST", "/api/games/mtg/cards", Some(json!({ "card": catalog("two", 2.0), "quantity": 1, "finish": "nonfoil" })));
    assert_eq!(get(&f.api, &format!("/api/games/mtg/lists?id={id}"))["entries"].as_array().unwrap().len(), 1);
}

/// One list's entries filter and sort like the collection's cards, and edits name
/// entries by their place in the list, so a filtered view never drops the others.
#[test]
fn a_list_filtered_sorted_and_edited() {
    let f = fixture();
    let (_, deck) = call(&f.api, "POST", "/api/games/mtg/lists", Some(json!({"name": "Burn"})));
    let id = deck["id"].as_str().unwrap().to_string();
    let path = |p: &str| format!("/api/games/mtg/lists{p}?id={id}");
    let shock = get(&f.api, &format!("/api/games/mtg/card?key={SHOCK}"))["card"].clone();
    call(&f.api, "POST", &path("/entries"), Some(json!({"card": shock, "quantity": 2})));
    call(&f.api, "POST", &path("/entries"), Some(json!({"card": catalog("aaaaaaaa-0000-4000-8000-000000000900", 5.0), "quantity": 3})));
    let names = |q: &str| -> Vec<String> {
        get(&f.api, &format!("/api/games/mtg/lists?id={id}{q}"))["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["card"]["name"].as_str().unwrap().to_string())
            .collect()
    };
    let card9 = "Card aaaaaaaa-0000-4000-8000-000000000900";
    assert_eq!(names(""), ["Shock", card9], "the list's own order");
    assert_eq!(names("&owned=missing"), [card9], "Shock's 2 are owned");
    assert_eq!(names("&owned=complete"), ["Shock"]);
    assert_eq!(names("&set=tst"), [card9]);
    assert_eq!(names("&q=shock"), ["Shock"]);
    assert_eq!(names("&price_min=1"), [card9]);
    assert_eq!(names("&sort=value&dir=desc"), [card9, "Shock"]);
    assert_eq!(names("&sort=name"), [card9, "Shock"]);
    let one = get(&f.api, &format!("/api/games/mtg/lists?id={id}&set=tst"));
    assert_eq!(one["list"]["cards"], 5, "totals are the whole list's");
    let sets: Vec<&str> = one["sets"].as_array().unwrap().iter().map(|s| s["Code"].as_str().unwrap()).collect();
    assert_eq!(sets, ["TST", "M19"], "by set name (the made-up TST has none)");
    assert_eq!(call(&f.api, "GET", &format!("/api/games/mtg/lists?id={id}&owned=some"), None).0, 400);

    // Edits by index: the second entry's quantity, then the first removed.
    let (status, t) = call(&f.api, "PUT", &path("/entries"), Some(json!({"changes": [{"index": 1, "quantity": 1, "finish": "Foil"}]})));
    assert_eq!((status, t["cards"].as_i64()), (200, Some(3)), "{t}");
    let (_, t) = call(&f.api, "PUT", &path("/entries"), Some(json!({"changes": [{"index": 0, "remove": true}]})));
    assert_eq!(t["cards"], 1);
    let left = &get(&f.api, &format!("/api/games/mtg/lists?id={id}"))["entries"][0];
    assert_eq!((left["card"]["name"].as_str(), left["finish"].as_str()), (Some(card9), Some("foil")));
    assert_eq!(call(&f.api, "PUT", &path("/entries"), Some(json!({"changes": [{"index": 5, "quantity": 1}]}))).0, 400);
    assert_eq!(call(&f.api, "PUT", &path("/entries"), Some(json!({"changes": [{"index": 0, "quantity": 0}]}))).0, 400);
}
