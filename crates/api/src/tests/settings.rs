//! Settings and opening links.

use super::*;

#[test]
fn settings_api() {
    let f = fixture();
    let got = get(&f.api, "/api/settings");
    assert_eq!(got["settings"]["theme"], "system");
    assert_eq!(got["choices"]["theme"].as_array().unwrap().len(), 3);
    // Only the settings sent change.
    let (status, got) = call(&f.api, "PUT", "/api/settings", Some(json!({"theme": "dark", "show_images": false})));
    assert_eq!(status, 200);
    assert_eq!((got["settings"]["theme"].as_str(), got["settings"]["show_images"].as_bool()), (Some("dark"), Some(false)));
    assert_eq!(got["settings"]["card_order"], "name");
    assert_eq!(call(&f.api, "PUT", "/api/settings", Some(json!({"theme": "purple"}))).0, 400);

    // The collection lists cards in the chosen order: by set, newest first, puts Core
    // Set 2019 (2018-07) before Dominaria (2018-04).
    call(&f.api, "PUT", "/api/settings", Some(json!({"card_order": "newest"})));
    assert_eq!(get(&f.api, "/api/games/mtg/cards")["cards"][0]["set"], "M19");
}

#[test]
fn links_open_in_the_system_browser() {
    let f = fixture();
    assert_eq!(get(&f.api, "/api/info")["opensLinks"], false);
    let (status, _) = call(&f.api, "POST", "/api/open", Some(json!({"url": "https://scryfall.com/card/m19/156"})));
    assert_eq!(status, 501, "--web opens links itself");

    let opened = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let o = opened.clone();
    let api = Api::new("TCG Binder", "test", Client::Desktop, Arc::new(SettingsStore::memory()), vec![f.svc.clone()]).with_url_opener(
        Arc::new(move |url| {
            o.lock().unwrap().push(url.to_string());
            Ok(())
        }),
    );
    assert_eq!(get(&api, "/api/info")["opensLinks"], true);
    let (status, v) = call(&api, "POST", "/api/open", Some(json!({"url": " https://scryfall.com/card/m19/156 "})));
    assert_eq!(status, 200, "{v}");
    for bad in ["javascript:alert(1)", "file:///etc/passwd", "tcgb://localhost/", "not a url", ""] {
        assert_eq!(call(&api, "POST", "/api/open", Some(json!({"url": bad}))).0, 400, "{bad}");
    }
    assert_eq!(*opened.lock().unwrap(), ["https://scryfall.com/card/m19/156"]);
}
