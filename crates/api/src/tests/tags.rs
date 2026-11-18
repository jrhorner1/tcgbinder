//! Tags through the API.

use super::*;

#[test]
fn tags_through_the_api() {
    let f = fixture();
    let names = |path: &str| -> Vec<String> {
        get(&f.api, path)["cards"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap().to_string()).collect()
    };
    // Shock: two purchases, tagged "Binder A" and "elves deck"; Opt: one, "binder a" (stored as "Binder A").
    let shock = json!({"purchases": [
        {"finish": "nonfoil", "condition": "NM", "quantity": 1, "price_paid": 0.25, "acquired_date": "2018-08-08", "tags": [" Binder  A "]},
        {"finish": "nonfoil", "condition": "NM", "quantity": 1, "price_paid": 0.25, "acquired_date": "2018-08-08", "tags": ["elves deck"]}
    ]});
    let (status, v) = call(&f.api, "PUT", &format!("/api/games/mtg/card?key={SHOCK}"), Some(shock));
    assert_eq!(status, 200, "{v}");
    let card = get(&f.api, &format!("/api/games/mtg/card?key={SHOCK}"))["card"].clone();
    let mut tags: Vec<Value> = card["purchases"].as_array().unwrap().iter().map(|p| p["tags"].clone()).collect();
    tags.sort_by_key(|t| t.to_string());
    assert_eq!(tags, [json!(["Binder A"]), json!(["elves deck"])], "cleaned and read back");
    let opt = json!({"purchases": [{"finish": "foil", "condition": "LP", "quantity": 1, "tags": ["binder a"]}]});
    call(&f.api, "PUT", &format!("/api/games/mtg/card?key={OPT}"), Some(opt));
    assert_eq!(
        get(&f.api, &format!("/api/games/mtg/card?key={OPT}"))["card"]["purchases"][0]["tags"],
        json!(["Binder A"]),
        "the existing spelling"
    );
    let (status, v) = call(
        &f.api,
        "PUT",
        &format!("/api/games/mtg/card?key={OPT}"),
        Some(json!({"purchases": [{"finish": "foil", "quantity": 1, "tags": ["a;b"]}]})),
    );
    assert_eq!(status, 400, "an invalid tag: {v}");
    assert!(v["error"].as_str().unwrap().contains("semicolon"));
    assert_eq!(get(&f.api, "/api/games/mtg/filters")["tags"], json!(["Binder A", "elves deck"]));

    // Filters, ignoring case; sort.
    assert_eq!(names("/api/games/mtg/cards?tags=binder%20a&sort=name"), ["Opt", "Shock"]);
    assert_eq!(names("/api/games/mtg/cards?tags=binder%20a,ELVES%20DECK"), ["Shock"], "all");
    assert_eq!(names("/api/games/mtg/cards?tags=binder%20a,elves%20deck&tags_mode=any&sort=name"), ["Opt", "Shock"]);
    assert_eq!(names("/api/games/mtg/cards?tags_not=elves%20deck"), ["Opt"]);
    assert!(names("/api/games/mtg/cards?untagged=1").is_empty());
    assert!(names("/api/games/mtg/cards?tags=nobody").is_empty());
    assert_eq!(call(&f.api, "GET", "/api/games/mtg/cards?tags_mode=some", None).0, 400);
    assert_eq!(names("/api/games/mtg/cards?sort=tag&dir=desc"), ["Opt", "Shock"], "both first tags are Binder A: a tie, so by name");
    call(
        &f.api,
        "PUT",
        &format!("/api/games/mtg/card?key={OPT}"),
        Some(json!({"purchases": [{"finish": "foil", "condition": "LP", "quantity": 1, "tags": ["zz"]}]})),
    );
    assert_eq!(names("/api/games/mtg/cards?sort=tag&dir=desc"), ["Opt", "Shock"], "zz before Binder A, reversed");
    assert_eq!(names("/api/games/mtg/cards?sort=tag"), ["Shock", "Opt"]);
    call(
        &f.api,
        "PUT",
        &format!("/api/games/mtg/card?key={OPT}"),
        Some(json!({"purchases": [{"finish": "foil", "condition": "LP", "quantity": 1, "tags": ["binder a"]}]})),
    );
    // The CSV export follows the same filters.
    let resp = f.api.handle(&request("GET", "/api/games/mtg/export?format=csv&tags=elves%20deck", Vec::new()));
    let csv = String::from_utf8(resp.body().clone()).unwrap();
    assert!(csv.contains("Shock") && !csv.contains("Opt,"), "{csv}");

    // Bulk add and remove, in the collection's spelling.
    let (status, v) =
        call(&f.api, "POST", "/api/games/mtg/bulk", Some(json!({"keys": [OPT], "add_tags": ["ELVES DECK"], "remove_tags": ["binder A"]})));
    assert_eq!(status, 200, "{v}");
    assert!(v["undo"]["backup"].as_str().is_some());
    assert_eq!(get(&f.api, &format!("/api/games/mtg/card?key={OPT}"))["card"]["purchases"][0]["tags"], json!(["elves deck"]));
    let many: Vec<String> = (0..21).map(|i| format!("t{i}")).collect();
    assert_eq!(call(&f.api, "POST", "/api/games/mtg/bulk", Some(json!({"keys": [OPT], "add_tags": many}))).0, 400);
    let twenty: Vec<String> = (0..20).map(|i| format!("t{i}")).collect();
    let before = get(&f.api, "/api/games/mtg/backups").as_array().unwrap().len();
    let (status, v) = call(&f.api, "POST", "/api/games/mtg/bulk", Some(json!({"keys": [OPT, SHOCK], "add_tags": twenty})));
    assert_eq!(status, 400, "each purchase has a tag already: 21 is too many. {v}");
    assert!(v["error"].as_str().unwrap().contains("Shock") || v["error"].as_str().unwrap().contains("Opt"), "{v}");
    assert_eq!(get(&f.api, "/api/games/mtg/backups").as_array().unwrap().len(), before, "refused before the backup");
    assert_eq!(call(&f.api, "POST", "/api/games/mtg/bulk", Some(json!({"keys": [OPT], "clear_tags": true}))).0, 200);

    // GET /tags: totals over only the tagged copies.
    call(&f.api, "POST", "/api/games/mtg/bulk", Some(json!({"keys": [OPT], "add_tags": ["elves deck"]})));
    let t = get(&f.api, "/api/games/mtg/tags");
    assert_eq!(t["tags"][0], json!({"name": "Binder A", "cards": 1, "copies": 1, "value": 0.2, "paid": 0.25}));
    assert_eq!(t["tags"][1]["cards"], 2);
    assert!(t["untagged"].is_null());

    // Rename into a new name, then into an existing tag (merging Shock's purchases), then delete.
    let rename = |from: &str, to: &str| call(&f.api, "PUT", &format!("/api/games/mtg/tags?name={}", enc(from)), Some(json!({"name": to})));
    assert_eq!(rename("nobody", "x").0, 404);
    assert_eq!(rename("elves deck", "a,b").0, 400);
    let (status, v) = rename("ELVES DECK", "Deck");
    assert_eq!((status, v["changed"].as_i64()), (200, Some(2)));
    assert!(v["undo"]["backup"].as_str().unwrap().contains("tag-rename"));
    let (_, v) = rename("deck", "binder a");
    assert_eq!(v["changed"], 2);
    let shock = get(&f.api, &format!("/api/games/mtg/card?key={SHOCK}"))["card"].clone();
    assert_eq!(shock["purchases"].as_array().unwrap().len(), 1, "made identical: merged");
    assert_eq!((shock["purchases"][0]["quantity"].as_i64(), &shock["purchases"][0]["tags"]), (Some(2), &json!(["Binder A"])));
    let (status, v) = call(&f.api, "DELETE", "/api/games/mtg/tags?name=binder%20A", None);
    assert_eq!((status, v["changed"].as_i64()), (200, Some(2)));
    assert!(get(&f.api, "/api/games/mtg/filters")["tags"].as_array().unwrap().is_empty());
    assert_eq!(get(&f.api, "/api/games/mtg/tags")["untagged"]["copies"], 3);
}
