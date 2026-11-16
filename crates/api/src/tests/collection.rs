//! The collection list, one card, editing, printings and bulk edits.

use super::*;

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
