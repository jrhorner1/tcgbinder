//! How requests are handled: the client header, and which hosts count as local.

use super::*;

#[test]
fn changes_need_the_client_header() {
    let f = fixture();
    // A cross-site form or fetch can't add the header, so it must be refused.
    for (method, path) in [("POST", "/api/games/mtg/cards".to_string()), ("DELETE", format!("/api/games/mtg/card?key={SHOCK}"))] {
        let req = http::Request::builder().method(method).uri(&path).body(b"{}".to_vec()).unwrap();
        assert_eq!(f.api.handle(&req).status(), 403, "{method} {path}");
    }
    assert_eq!(f.svc.store.len(), 2);
}

#[test]
fn local_hosts() {
    for (host, want) in [
        ("localhost:8080", true),
        ("127.0.0.1:8080", true),
        ("[::1]:8080", true),
        ("::1", true),
        ("LOCALHOST", true),
        ("evil.example:8080", false),
        ("attacker.com", false),
        ("localhost.attacker.com", false),
    ] {
        assert_eq!(is_local_host(host), want, "{host}");
    }
}
