//! The watchlist and its alerts.

use super::*;

#[test]
fn watchlist_and_alerts() {
    let f = fixture();
    let heard = Arc::new(std::sync::Mutex::new(Vec::new()));
    let h = heard.clone();
    f.svc.set_alert_hook(Box::new(move |game, w| h.lock().unwrap().push(format!("{game} {} {:?}", w.card.name, w.price()))));
    let shock = get(&f.api, &format!("/api/games/mtg/card?key={SHOCK}"))["card"].clone();
    let watch = |body: Value| call(&f.api, "POST", "/api/games/mtg/watchlist", Some(body));

    let (status, w) = watch(json!({"card": shock, "finish": "nonfoil", "rule": {"kind": "percent", "value": 20}}));
    assert_eq!(status, 200, "{w}");
    assert_eq!((w["baseline"].as_f64(), w["key"].as_str()), (Some(0.2), Some(format!("{SHOCK}/nonfoil").as_str())));
    assert_eq!(watch(json!({"card": shock, "finish": "nonfoil"})).0, 400, "already watched");
    assert_eq!(watch(json!({"card": shock, "finish": "etched"})).0, 400, "no price in that finish for a percent rule");
    assert_eq!(watch(json!({"card": shock, "finish": "foil", "rule": {"kind": "above", "value": 0}})).0, 400);
    let (status, _) = watch(json!({"card": shock, "finish": "foil", "rule": {"kind": "below", "value": 0.5}}));
    assert_eq!(status, 200);

    // The fake catalog prices every card at $0.25, non-foil only: non-foil Shock moves
    // 25% (an alert); foil Shock now has no price, so it can't cross.
    f.svc.refresh_prices(&mut |_, _| {}).unwrap();
    assert_eq!(*heard.lock().unwrap(), ["mtg Shock Some(0.25)"]);
    let home = get(&f.api, "/api/games/mtg/home");
    assert_eq!(home["alerts"].as_array().unwrap().len(), 1);
    let list = get(&f.api, "/api/games/mtg/watchlist");
    let nonfoil = list.as_array().unwrap().iter().find(|w| w["finish"] == "nonfoil").unwrap().clone();
    assert_eq!((nonfoil["change"].as_f64(), nonfoil["changePercent"].as_f64()), (Some(0.05), Some(25.0)));
    assert_eq!(nonfoil["triggered"]["price"].as_f64(), Some(0.25));

    let key = enc(nonfoil["key"].as_str().unwrap());
    let path = format!("/api/games/mtg/watchlist?key={key}");
    let (status, w) = call(&f.api, "PUT", &path, Some(json!({"action": "dismiss"})));
    assert_eq!((status, w["triggered"].is_null(), w["baseline"].as_f64()), (200, true, Some(0.2)));
    let (_, w) = call(&f.api, "PUT", &path, Some(json!({"action": "reset"})));
    assert_eq!((w["baseline"].as_f64(), w["change"].as_f64()), (Some(0.25), Some(0.0)));
    let (_, w) = call(&f.api, "PUT", &path, Some(json!({"rule": {"kind": "above", "value": 2}})));
    assert_eq!(w["rule"], json!({"kind": "above", "value": 2.0}));
    assert_eq!(call(&f.api, "PUT", &path, Some(json!({"action": "snooze"}))).0, 400);
    assert!(get(&f.api, "/api/games/mtg/home")["alerts"].as_array().unwrap().is_empty());
    assert_eq!(call(&f.api, "DELETE", &path, None).0, 200);
    assert_eq!(call(&f.api, "DELETE", &path, None).0, 404);
    assert_eq!(get(&f.api, "/api/games/mtg/watchlist").as_array().unwrap().len(), 1);
}
