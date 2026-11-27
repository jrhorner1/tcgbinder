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

#[test]
fn a_list_of_owned_cards_into_a_lot() {
    let f = fixture();
    let new_lot = |name: &str| {
        let (status, lot) = call(
            &f.api,
            "POST",
            "/api/games/mtg/lots",
            Some(
                json!({"name": name, "total_paid": 20, "acquired_date": "2018-08-01", "count": {"kind": "known", "cards": 125}, "split": "even"}),
            ),
        );
        assert_eq!(status, 200, "{lot}");
        lot["id"].as_str().unwrap().to_string()
    };
    // One of the two Shocks is in another lot already.
    let other = new_lot("Other box");
    let (status, _) =
        call(&f.api, "POST", &format!("/api/games/mtg/lots/cards?id={other}"), Some(json!({"cards": [{"key": SHOCK, "quantity": 1}]})));
    assert_eq!(status, 200);

    let toolkit = new_lot("Deck builder's toolkit");
    let path = format!("/api/games/mtg/lots/list?id={toolkit}");
    let list = "Deck\n2 Shock\n1 Opt *F*\n1 Opt\n3 Lightning Bolt (M19) 1\n";
    let (status, v) = call(&f.api, "POST", &path, Some(json!({ "list": list })));
    assert_eq!(status, 200, "{v}");
    let lines = v["lines"].as_array().unwrap();
    assert_eq!(lines.len(), 4, "the heading is skipped");
    let found = |i: usize| lines[i]["found"].as_array().unwrap().iter().map(|x| x["quantity"].as_i64().unwrap()).sum::<i64>();
    assert_eq!(
        (found(0), lines[0]["inLots"].as_i64(), lines[0]["missing"].as_str()),
        (1, Some(1), Some("1 Shock")),
        "the lot's Shock is left"
    );
    assert_eq!(found(1), 1, "the foil Opt");
    assert_eq!((found(2), lines[2]["missing"].as_str()), (0, Some("1 Opt")), "no nonfoil Opt is owned");
    assert_eq!(lines[3]["missing"], "3 Lightning Bolt (M19) 1");
    assert_eq!((v["found"].as_i64(), v["missing"].as_str()), (Some(2), Some("1 Shock\n1 Opt\n3 Lightning Bolt (M19) 1")));
    assert_eq!(get(&f.api, &format!("/api/games/mtg/lots?id={toolkit}"))["lot"]["copies"], 0, "a preview changes nothing");

    let (status, v) = call(&f.api, "POST", &path, Some(json!({ "list": list, "apply": true })));
    assert_eq!(status, 200, "{v}");
    assert_eq!((v["moved"].as_i64(), v["lot"]["copies"].as_i64()), (Some(2), Some(2)));
    assert!(v["undo"]["backup"].as_str().is_some());
    assert_eq!(lot_purchases(&f, SHOCK, &json!(other)).iter().map(|p| p.2).sum::<i64>(), 1, "still in the other lot");
    assert_eq!(lot_purchases(&f, SHOCK, &json!(toolkit)), [(0.16, "2018-08-01".to_string(), 1)], "$20 / 125, at the lot's date");
    // Again: everything owned is in a lot now, so nothing moves.
    let (_, v) = call(&f.api, "POST", &path, Some(json!({ "list": list, "apply": true })));
    assert_eq!((v["moved"].as_i64(), v["found"].as_i64()), (Some(0), Some(0)));

    assert_eq!(call(&f.api, "POST", &path, Some(json!({ "list": "\n// nothing\n" }))).0, 400);
    assert_eq!(call(&f.api, "POST", "/api/games/mtg/lots/list?id=nope", Some(json!({ "list": "1 Shock" }))).0, 404);
}

#[test]
fn backups_and_collection_files_keep_lots() {
    let f = fixture();
    let (_, lot) = call(
        &f.api,
        "POST",
        "/api/games/mtg/lots",
        Some(
            json!({"name": "Toolkit", "total_paid": 4, "acquired_date": "2018-08-01", "count": {"kind": "known", "cards": 2}, "split": "even"}),
        ),
    );
    let id = lot["id"].as_str().unwrap().to_string();
    call(&f.api, "POST", &format!("/api/games/mtg/lots/cards?id={id}"), Some(json!({"cards": [{"key": SHOCK}]})));

    // The JSON export carries the lot.
    let file = f.api.handle(&request("GET", "/api/games/mtg/export?format=json", Vec::new())).body().clone();
    let exported: Value = serde_json::from_slice(&file).unwrap();
    assert_eq!(exported["lots"][0]["name"], "Toolkit");

    // Deleting the lot (keeping its cards) can be undone: the lot and the cards' lot come back.
    let (status, v) = call(&f.api, "DELETE", &format!("/api/games/mtg/lots?id={id}&cards=keep"), None);
    assert_eq!(status, 200, "{v}");
    assert!(f.svc.lots.get(&id).is_none());
    let backups = get(&f.api, "/api/games/mtg/backups");
    assert_eq!((backups[0]["reason"].as_str(), backups[0]["lots"].as_i64()), (Some("lot-delete"), Some(1)));
    let (status, r) = call(&f.api, "POST", "/api/games/mtg/backups/restore", Some(json!({ "name": v["undo"]["backup"] })));
    assert_eq!(status, 200, "{r}");
    assert_eq!(f.svc.lots.get(&id).map(|l| l.name), Some("Toolkit".to_string()));
    assert_eq!(lot_purchases(&f, SHOCK, &json!(id)), [(2.0, "2018-08-01".to_string(), 2)]);
    // Restoring the backup taken by the restore goes back to no lot again.
    call(&f.api, "POST", "/api/games/mtg/backups/restore", Some(json!({ "name": r["undo"]["backup"] })));
    assert!(f.svc.lots.get(&id).is_none());

    // Importing the collection file into a collection without the lot adds it.
    let g = fixture();
    let resp = g.api.handle(&request("POST", "/api/games/mtg/import?preview=1", file.clone()));
    let p: Value = serde_json::from_slice(resp.body()).unwrap();
    assert!(p["notes"].as_array().unwrap().iter().any(|n| n.as_str().unwrap().contains("Toolkit")), "{p}");
    let (status, _) = call(&g.api, "POST", "/api/games/mtg/import/commit", Some(json!({ "token": p["token"] })));
    assert_eq!(status, 200);
    assert_eq!(g.svc.lots.get(&id).map(|l| l.total_paid), Some(4.0));
    // Imported again: the lot is already there, so it isn't added twice.
    let resp = g.api.handle(&request("POST", "/api/games/mtg/import?preview=1", file));
    let p: Value = serde_json::from_slice(resp.body()).unwrap();
    assert!(p["notes"].as_array().unwrap().is_empty(), "{p}");
    assert_eq!(g.svc.lots.all().len(), 1);
}

#[test]
fn copies_in_a_lost_lot() {
    let f = fixture();
    let (_, lot) = call(
        &f.api,
        "POST",
        "/api/games/mtg/lots",
        Some(
            json!({"name": "Bundle", "total_paid": 3, "acquired_date": "2018-08-01", "count": {"kind": "known", "cards": 2}, "split": "even"}),
        ),
    );
    let id = lot["id"].as_str().unwrap().to_string();
    call(&f.api, "POST", &format!("/api/games/mtg/lots/cards?id={id}"), Some(json!({"cards": [{"key": SHOCK}]})));
    assert!(get(&f.api, "/api/games/mtg/lots/lost")["lots"].as_array().unwrap().is_empty());
    // The lot's record goes (as when an older backup brings back its copies' lot).
    let forget = || {
        f.svc
            .lots
            .update_all(|all| {
                all.retain(|l| l.id != id);
                Ok(())
            })
            .unwrap()
    };
    forget();

    let lost = get(&f.api, "/api/games/mtg/lots/lost");
    let l = &lost["lots"][0];
    assert_eq!(
        (l["id"].as_str(), l["cards"].as_i64(), l["copies"].as_i64(), l["paid"].as_f64(), l["first_date"].as_str(), l["names"][0].as_str()),
        (Some(id.as_str()), Some(1), Some(2), Some(3.0), Some("2018-08-01"), Some("Shock"))
    );
    // Recovered: a finished lot again, paid what its copies were costed at; their prices stay.
    let (status, r) = call(&f.api, "POST", "/api/games/mtg/lots/lost", Some(json!({ "id": id, "action": "recover" })));
    assert_eq!(status, 200, "{r}");
    assert_eq!(
        (r["lot"]["name"].as_str(), r["lot"]["total_paid"].as_f64(), r["lot"]["open"].as_bool()),
        (Some("Recovered lot (2018-08-01)"), Some(3.0), Some(false))
    );
    assert_eq!(r["lot"]["copies"], 2);
    assert_eq!(lot_purchases(&f, SHOCK, &json!(id)), [(1.5, "2018-08-01".to_string(), 2)]);
    assert!(get(&f.api, "/api/games/mtg/lots/lost")["lots"].as_array().unwrap().is_empty());

    // Released instead: the copies leave the lot and keep their price, with Undo.
    forget();
    let (status, r) = call(&f.api, "POST", "/api/games/mtg/lots/lost", Some(json!({ "id": id, "action": "release" })));
    assert_eq!((status, r["freed"].as_i64()), (200, Some(1)), "{r}");
    assert!(r["undo"]["backup"].is_string());
    let p = get(&f.api, &format!("/api/games/mtg/card?key={SHOCK}"))["card"]["purchases"][0].clone();
    assert_eq!((p["lot"].as_str(), p["price_paid"].as_f64()), (Some(""), Some(1.5)));

    assert_eq!(call(&f.api, "POST", "/api/games/mtg/lots/lost", Some(json!({ "id": id, "action": "recover" }))).0, 404, "nothing lost now");
}
