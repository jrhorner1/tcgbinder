//! Sealed product.

use super::*;

#[test]
fn sealed_product() {
    let f = fixture();
    let today = tcgbinder_core::dates::today_string();
    assert!(get(&f.api, "/api/games/mtg/home")["sealed"].is_null(), "none yet");
    let add = |body: Value| call(&f.api, "POST", "/api/games/mtg/sealed", Some(body));
    let (status, bx) = add(json!({
        "product_uuid": "sp-box", "name": "Core Set 2019 Booster Box", "set": "m19", "category": "booster_box", "subtype": "draft",
        "quantity": 2, "price_paid": 90, "acquired_date": "2018-07-20", "value": 150, "tcgplayer_product_id": 166550,
        "purchase_url": "https://mtgjson.com/links/abc"
    }));
    assert_eq!(status, 200, "{bx}");
    assert_eq!(
        (bx["set"].as_str(), bx["value_date"].as_str(), bx["valueStale"].as_bool()),
        (Some("M19"), Some(today.as_str()), Some(false))
    );
    assert_eq!(bx["gain"].as_f64(), Some(120.0), "(150 - 90) x 2");
    let (_, typed) = add(json!({"name": "Prerelease kit", "quantity": 1, "price_paid": 30}));
    assert_eq!((typed["valueStale"].as_bool(), typed["gain"].is_null()), (Some(true), true), "no value yet");
    assert_eq!(add(json!({"name": " ", "quantity": 1})).0, 400);
    assert_eq!(add(json!({"name": "x", "quantity": 0})).0, 400);
    assert_eq!(add(json!({"name": "x", "quantity": 1, "acquired_date": "July"})).0, 400);

    let all = get(&f.api, "/api/games/mtg/sealed");
    assert_eq!(all["totals"], json!({"items": 3, "paid": 210.0, "value": 300.0, "unvalued": 1, "stale": 1}));
    assert_eq!(all["items"][0]["name"], "Prerelease kit", "by set: no set first");
    let home = get(&f.api, "/api/games/mtg/home");
    assert_eq!(home["sealed"], json!({"value": 300.0, "items": 3, "unvalued": 1}));
    assert_eq!(home["marketValue"].as_f64(), Some(0.40), "the cards' value stays cards-only");

    // A value typed later is dated today; an old value is marked.
    let path = format!("/api/games/mtg/sealed?id={}", typed["id"].as_str().unwrap());
    let mut changed = typed.clone();
    changed["value"] = json!(45);
    let (status, v) = call(&f.api, "PUT", &path, Some(changed));
    assert_eq!((status, v["value_date"].as_str()), (200, Some(today.as_str())));
    let mut old = v.clone();
    old["value_date"] = json!("2020-01-01");
    let (_, v) = call(&f.api, "PUT", &path, Some(old));
    assert_eq!((v["value_date"].as_str(), v["valueStale"].as_bool()), (Some("2020-01-01"), Some(true)));

    assert_eq!(call(&f.api, "GET", "/api/games/mtg/sealed/products?set=", None).0, 400);
    assert_eq!(get(&f.api, "/api/games/mtg/sealed/products?set=m19")["products"], json!([]), "the fake has no MTGJSON set files");
    assert_eq!(call(&f.api, "DELETE", &path, None).0, 200);
    assert_eq!(call(&f.api, "DELETE", &path, None).0, 404);
    assert_eq!(get(&f.api, "/api/games/mtg/sealed")["items"].as_array().unwrap().len(), 1);
}
