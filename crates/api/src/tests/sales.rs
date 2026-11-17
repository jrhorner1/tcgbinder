//! Selling copies and the earnings ledger.

use super::*;

#[test]
fn sell_and_earnings() {
    let f = fixture();
    let sell = |key: &str, body: Value| call(&f.api, "POST", &format!("/api/games/mtg/card/sell?key={key}"), Some(body));
    let year = &tcgbinder_core::dates::today_string()[..4];
    // Shock: 2 non-foil NM bought at $0.25 on 2018-08-08.
    let shock = json!({"finish": "nonfoil", "condition": "NM", "price_paid": 0.25, "acquired_date": "2018-08-08"});
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
fn selling_tagged_copies_and_undoing() {
    let f = fixture();
    // Shock: 2 copies tagged "binder A", 1 tagged "for sale", same price and date.
    let body = json!({"purchases": [
        {"finish": "nonfoil", "condition": "NM", "quantity": 2, "price_paid": 0.25, "acquired_date": "2018-08-08", "tags": ["binder A"]},
        {"finish": "nonfoil", "condition": "NM", "quantity": 1, "price_paid": 0.25, "acquired_date": "2018-08-08", "tags": ["for sale"]}
    ]});
    assert_eq!(call(&f.api, "PUT", &format!("/api/games/mtg/card?key={SHOCK}"), Some(body)).0, 200);
    let sell = json!({"finish": "nonfoil", "condition": "NM", "price_paid": 0.25, "acquired_date": "2018-08-08", "tags": ["FOR SALE"],
        "quantity": 1, "sale_each": 1.0});
    let (status, v) = call(&f.api, "POST", &format!("/api/games/mtg/card/sell?key={SHOCK}"), Some(sell));
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["sale"]["purchase_tags"], json!(["for sale"]), "the tagged purchase was the one sold");
    let tags = |f: &Fixture| -> Vec<(i64, Value)> {
        let mut out: Vec<(i64, Value)> = get(&f.api, &format!("/api/games/mtg/card?key={SHOCK}"))["card"]["purchases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| (p["quantity"].as_i64().unwrap(), p["tags"].clone()))
            .collect();
        out.sort_by_key(|(q, t)| (*q, t.to_string()));
        out
    };
    assert_eq!(tags(&f), [(2, json!(["binder A"]))]);
    let id = v["sale"]["id"].as_str().unwrap().to_string();
    assert_eq!(call(&f.api, "DELETE", &format!("/api/games/mtg/sales?id={id}&restore=1"), None).0, 200);
    assert_eq!(tags(&f), [(1, json!(["for sale"])), (2, json!(["binder A"]))], "back with its tags");
    // Statistics count each tag's copies.
    let stats = get(&f.api, "/api/games/mtg/stats");
    assert_eq!(stats["tags"]["tags"][1], json!({"name": "for sale", "cards": 1, "copies": 1, "value": 0.2, "paid": 0.25}));
    assert_eq!(stats["tags"]["untagged"]["copies"], 1, "Opt");
}

#[test]
fn selling_from_a_lot_and_undoing() {
    let f = fixture();
    let (_, lot) = call(&f.api, "POST", "/api/games/mtg/lots", Some(json!({ "name": "Box", "total_paid": 4.0, "split": "even" })));
    let id = lot["id"].clone();
    add_in_lot(&f, "lot-a", 1.0, 2, &id);
    let p = get(&f.api, "/api/games/mtg/card?key=lot-a")["card"]["purchases"][0].clone();
    let sell = json!({"finish": "nonfoil", "condition": "NM", "price_paid": p["price_paid"], "acquired_date": p["acquired_date"], "tags": [],
        "quantity": 1, "sale_each": 3.0});
    let (status, v) = call(&f.api, "POST", "/api/games/mtg/card/sell?key=lot-a", Some(sell));
    assert_eq!(status, 200, "{v}");
    assert_eq!((v["sale"]["lot"].clone(), v["sale"].get("purchase_tags")), (id.clone(), None), "shown as the lot, not a tag");
    assert_eq!(get(&f.api, "/api/games/mtg/sales")["sales"][0]["lot"], id);
    let sale = v["sale"]["id"].as_str().unwrap().to_string();
    assert_eq!(call(&f.api, "DELETE", &format!("/api/games/mtg/sales?id={sale}&restore=1"), None).0, 200);
    assert_eq!(lot_purchases(&f, "lot-a", &id).iter().map(|p| p.2).sum::<i64>(), 2, "back in the lot");
}
