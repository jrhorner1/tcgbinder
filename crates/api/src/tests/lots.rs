//! Lots: adding cards in one, owned cards in and out, finishing and deleting.

use super::*;

#[test]
fn owned_cards_into_and_out_of_lots() {
    let f = fixture();
    // A finished lot of 4 cards for $8, split evenly: $2 each.
    let (status, lot) = call(
        &f.api,
        "POST",
        "/api/games/mtg/lots",
        Some(
            json!({"name": "Old box", "total_paid": 8, "acquired_date": "2020-01-01", "count": {"kind": "known", "cards": 4}, "split": "even"}),
        ),
    );
    assert_eq!(status, 200, "{lot}");
    let id = lot["id"].as_str().unwrap().to_string();
    let path = format!("/api/games/mtg/lots/cards?id={id}");
    // Shock (2 copies at $0.25) and one Opt.
    let (status, v) = call(&f.api, "POST", &path, Some(json!({"cards": [{"key": SHOCK}, {"key": OPT, "quantity": 1}]})));
    assert_eq!(status, 200, "{v}");
    assert_eq!((v["moved"].as_i64(), v["lot"]["entered"].as_i64()), (Some(3), Some(3)));
    assert!(v["undo"]["backup"].as_str().is_some());
    let shock = get(&f.api, &format!("/api/games/mtg/card?key={SHOCK}"))["card"].clone();
    assert_eq!(
        (shock["purchases"][0]["lot"].as_str(), shock["purchases"][0]["price_paid"].as_f64()),
        (Some(id.as_str()), Some(2.0)),
        "repriced"
    );
    assert_eq!(shock["purchases"][0]["acquired_date"], "2020-01-01", "the lot's date");
    assert_eq!(call(&f.api, "POST", &path, Some(json!({"cards": [{"key": SHOCK}]}))).0, 400, "already all in the lot");
    assert_eq!(call(&f.api, "POST", &path, Some(json!({"cards": []}))).0, 400);
    assert_eq!(call(&f.api, "POST", "/api/games/mtg/lots/cards?id=nope", Some(json!({"cards": [{"key": OPT}]}))).0, 404);

    // Out again: Shock stays, at the lot's price; the lot has Opt left.
    let (status, v) = call(&f.api, "DELETE", &format!("{path}&key={SHOCK}"), None);
    assert_eq!((status, v["freed"].as_i64(), v["lot"]["entered"].as_i64()), (200, Some(2), Some(1)));
    let shock = get(&f.api, &format!("/api/games/mtg/card?key={SHOCK}"))["card"].clone();
    assert_eq!(
        (shock["copy_count"].as_i64(), shock["purchases"][0]["lot"].as_str(), shock["purchases"][0]["price_paid"].as_f64()),
        (Some(2), Some(""), Some(2.0))
    );
    assert_eq!(call(&f.api, "DELETE", &format!("{path}&key={SHOCK}"), None).0, 400);
}

#[test]
fn lots_are_stored_as_tags() {
    let f = fixture();
    let (_, lot) = call(&f.api, "POST", "/api/games/mtg/lots", Some(json!({ "name": "Box", "total_paid": 4.0, "split": "even" })));
    let id = lot["id"].clone();
    let lot_id = id.as_str().unwrap();
    add_in_lot(&f, "lot-a", 1.0, 2, &id);
    // Stored as a tag, not a field; shown as `lot` and kept out of the tags.
    let text = std::fs::read_to_string(f.svc.store.file_path()).unwrap();
    assert!(text.contains(&format!("\"lot:{lot_id}\"")) && !text.contains("\"lot\":"), "{text}");
    let p = get(&f.api, "/api/games/mtg/card?key=lot-a")["card"]["purchases"][0].clone();
    assert_eq!((p["lot"].as_str(), p["tags"].clone()), (Some(lot_id), json!([])));
    let t = get(&f.api, "/api/games/mtg/tags");
    assert!(t["tags"].as_array().unwrap().is_empty(), "{t}");
    assert_eq!(t["untagged"]["copies"].as_i64(), Some(5), "the lot's 2 and the fixture's 3");
    assert!(get(&f.api, "/api/games/mtg/filters")["tags"].as_array().unwrap().is_empty());
    let names = |path: &str| -> Vec<String> {
        get(&f.api, path)["cards"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap().to_string()).collect()
    };
    assert_eq!(names(&format!("/api/games/mtg/cards?lot={lot_id}")).len(), 1, "filtered by lot=");
    assert_eq!(names(&format!("/api/games/mtg/cards?q={lot_id}")).len(), 0, "the lot's tag isn't searched");

    // Clearing tags keeps the card in its lot.
    let (status, _) = call(&f.api, "POST", "/api/games/mtg/bulk", Some(json!({"keys": ["lot-a"], "clear_tags": true})));
    assert_eq!(status, 200);
    assert_eq!(get(&f.api, "/api/games/mtg/card?key=lot-a")["card"]["purchases"][0]["lot"].as_str(), Some(lot_id));

    // Tags starting "lot:" are refused wherever tags come in.
    let bad = json!(["lot:x"]);
    let put = json!({"purchases": [{"finish": "nonfoil", "quantity": 1, "tags": bad}]});
    let add = json!({ "card": catalog("lot-b", 1.0), "quantity": 1, "finish": "nonfoil", "tags": bad });
    for (method, path, body) in [
        ("PUT", format!("/api/games/mtg/card?key={SHOCK}"), put),
        ("POST", "/api/games/mtg/cards".to_string(), add),
        ("POST", "/api/games/mtg/bulk".to_string(), json!({"keys": [SHOCK], "add_tags": bad})),
        ("POST", "/api/games/mtg/bulk".to_string(), json!({"keys": [SHOCK], "remove_tags": bad})),
        ("POST", "/api/games/mtg/import/commit".to_string(), json!({"token": preview(&f)["token"], "tags": bad})),
    ] {
        let (status, v) = call(&f.api, method, &path, Some(body));
        assert_eq!(status, 400, "{method} {path}: {v}");
        assert!(v["error"].as_str().unwrap().contains("can't start with 'lot:'"), "{method} {path}: {v}");
    }
    call(
        &f.api,
        "PUT",
        &format!("/api/games/mtg/card?key={SHOCK}"),
        Some(json!({"purchases": [{"finish": "nonfoil", "quantity": 1, "tags": ["deck"]}]})),
    );
    let (status, v) = call(&f.api, "PUT", "/api/games/mtg/tags?name=deck", Some(json!({"name": "lot:x"})));
    assert_eq!(status, 400, "{v}");
    assert!(v["error"].as_str().unwrap().contains("can't start with 'lot:'"));
    assert_eq!(call(&f.api, "DELETE", &format!("/api/games/mtg/tags?name=lot:{lot_id}"), None).0, 404, "not a tag the user has");
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
fn editing_a_card_reprices_its_lots() {
    let f = fixture();
    // An even lot of 4 cards for $8, finished: $2 a card.
    let (_, lot) = call(
        &f.api,
        "POST",
        "/api/games/mtg/lots",
        Some(
            json!({"name": "Box", "total_paid": 8, "acquired_date": "2020-01-01", "count": {"kind": "known", "cards": 4}, "split": "even"}),
        ),
    );
    let id = lot["id"].as_str().unwrap().to_string();
    // Move Opt's copy into the lot by saving the card with the lot set, as the collection list does.
    let opt = get(&f.api, &format!("/api/games/mtg/card?key={OPT}"))["card"].clone();
    let mut p = opt["purchases"][0].clone();
    p["lot"] = json!(id);
    let (status, v) = call(&f.api, "PUT", &format!("/api/games/mtg/card?key={OPT}"), Some(json!({ "purchases": [p] })));
    assert_eq!(status, 200, "{v}");
    let p = get(&f.api, &format!("/api/games/mtg/card?key={OPT}"))["card"]["purchases"][0].clone();
    assert_eq!(
        (p["lot"].as_str(), p["price_paid"].as_f64(), p["acquired_date"].as_str()),
        (Some(id.as_str()), Some(2.0), Some("2020-01-01"))
    );
    let lots = get(&f.api, "/api/games/mtg/lots");
    assert_eq!(lots[0]["entered"].as_i64(), Some(1));

    // Cards in no lot: Shock; Opt is now only in the lot.
    let names: Vec<String> = get(&f.api, "/api/games/mtg/cards?lot=none")["cards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap().into())
        .collect();
    assert_eq!(names, ["Shock"]);

    // Deleting the card takes its copy out of the lot's count.
    assert_eq!(call(&f.api, "DELETE", &format!("/api/games/mtg/card?key={OPT}"), None).0, 200);
    assert_eq!(get(&f.api, "/api/games/mtg/lots")[0]["entered"].as_i64(), Some(0));
}
