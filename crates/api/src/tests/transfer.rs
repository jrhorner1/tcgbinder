//! Import, export, backups and jobs.

use super::*;

/// Downloads an export, returning its file name and body.
fn download(api: &Api, path: &str) -> (String, String) {
    let resp = api.handle(&request("GET", path, Vec::new()));
    assert_eq!(resp.status(), 200, "{path}");
    let disposition = resp.headers()[header::CONTENT_DISPOSITION].to_str().unwrap().to_string();
    (disposition, String::from_utf8(resp.body().clone()).unwrap())
}

/// Exports the collection and imports it back: every card merges, doubling the
/// quantities.
#[test]
fn export_import_round_trip() {
    let f = fixture();
    let resp = f.api.handle(&request("GET", "/api/games/mtg/export", Vec::new()));
    assert!(resp.headers()[header::CONTENT_DISPOSITION].to_str().unwrap().contains("attachment"));
    let data = resp.body().clone();
    assert!(is_app_file(&data));

    let mut body =
        b"--XyZ\r\nContent-Disposition: form-data; name=\"file\"; filename=\"backup.json\"\r\nContent-Type: application/json\r\n\r\n"
            .to_vec();
    body.extend_from_slice(&data);
    body.extend_from_slice(b"\r\n--XyZ--\r\n");
    let req = http::Request::builder()
        .method("POST")
        .uri("/api/games/mtg/import")
        .header(CLIENT_HEADER, "test")
        .header(header::CONTENT_TYPE, "multipart/form-data; boundary=XyZ")
        .body(body)
        .unwrap();
    let resp = f.api.handle(&req);
    let res: Value = serde_json::from_slice(resp.body()).unwrap();
    assert_eq!(resp.status(), 200, "{res}");
    assert_eq!((res["cards"].as_i64(), res["newCards"].as_i64(), res["alreadyOwned"].as_i64()), (Some(3), Some(0), Some(2)));
    assert_eq!(res["format"], APP_FILE_FORMAT);
    assert_eq!(f.svc.store.cards().iter().map(Card::copy_count).sum::<i64>(), 6);

    // The raw body works too, and a revision change tells open pages to refresh.
    let before = get(&f.api, "/api/status")["revision"].as_u64().unwrap();
    let (status, _) = call(&f.api, "POST", "/api/games/mtg/import", None);
    assert_eq!(status, 502); // an empty file
    let resp = f.api.handle(&request("POST", "/api/games/mtg/import", data));
    assert_eq!(resp.status(), 200);
    assert!(get(&f.api, "/api/status")["revision"].as_u64().unwrap() > before);

    // Apps that can't download files save the export instead.
    assert_eq!(call(&f.api, "POST", "/api/games/mtg/export", None).0, 404);
    let out = tempfile::tempdir().unwrap();
    let api = Api::new("TCG Binder", "test", Client::Phone, Arc::new(SettingsStore::memory()), vec![f.svc.clone()])
        .with_export_dir(out.path().join("exports"));
    let (status, saved) = call(&api, "POST", "/api/games/mtg/export", None);
    assert_eq!(status, 200);
    assert!(is_app_file(&std::fs::read(saved["path"].as_str().unwrap()).unwrap()));
    assert_eq!(get(&api, "/api/info")["saveExports"], true);

    // The desktop asks where to save: the Save dialog starts in the export folder with
    // the file's name, and saves where chosen; cancelling saves nothing.
    let chosen = out.path().join("my export.csv");
    let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
    let (to, seen) = (chosen.clone(), asked.clone());
    let api = Api::new("TCG Binder", "test", Client::Desktop, Arc::new(SettingsStore::memory()), vec![f.svc.clone()])
        .with_export_dir(out.path().join("Downloads"))
        .with_save_chooser(Arc::new(move |dir, name| {
            seen.lock().unwrap().push((dir.to_path_buf(), name.to_string()));
            name.ends_with(".csv").then(|| to.clone())
        }));
    let (status, saved) = call(&api, "POST", "/api/games/mtg/export?format=csv", None);
    assert_eq!((status, saved["path"].as_str()), (200, chosen.to_str()));
    assert!(std::fs::read_to_string(&chosen).unwrap().starts_with("Name,"));
    let (dir, name) = asked.lock().unwrap()[0].clone();
    assert_eq!(dir, out.path().join("Downloads"));
    assert!(name.starts_with("mtg-collection-") && name.ends_with(".csv"), "{name}");
    let (status, v) = call(&api, "POST", "/api/games/mtg/export?format=json", None);
    assert_eq!((status, v["cancelled"].as_bool()), (200, Some(true)));
    assert!(!out.path().join("Downloads").exists(), "nothing saved, no folder made");
}

#[test]
fn csv_export_reads_back() {
    let f = fixture();
    let mut shock = f.svc.store.get(SHOCK).unwrap();
    shock.add_purchase(
        "foil",
        Condition::LightPlay,
        Purchase { quantity: 1, tags: vec!["binder A".into(), "for sale".into()], ..Default::default() },
    );
    f.svc.store.save_card(SHOCK, shock).unwrap();

    let (name, csv) = download(&f.api, "/api/games/mtg/export?format=csv");
    assert!(name.contains(".csv"), "{name}");
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines[0], format!("{},Scryfall ID", tcgbinder_core::collection::CSV_COLUMNS.join(",")));
    assert_eq!(lines.len(), 4, "a row per purchase: {csv}");
    assert!(lines.contains(&format!("Shock,M19,Core Set 2019,156,en,nonfoil,NM,2,0.25,2018-08-08,,common,0.2,{SHOCK}").as_str()), "{csv}");

    // The list's filters and sort apply.
    let (_, red) = download(&f.api, "/api/games/mtg/export?format=csv&f.Color=R&sort=name");
    assert!(red.lines().skip(1).all(|l| l.starts_with("Shock,")), "{red}");
    let (_, blue) = download(&f.api, "/api/games/mtg/export?format=csv&f.Color=U");
    assert_eq!(blue.lines().count(), 2);

    // Imported into an empty collection, the same purchases come back.
    let (scryfall, _fake) = testing::search_server();
    let g = Magic::with_clients(scryfall, mtgjson::Client::new(&_fake.url));
    let dir = tempfile::tempdir().unwrap();
    let empty = GameService::from_store(Box::new(g), Store::open(dir.path().join("collection.json"), ID).unwrap()).unwrap();
    let res = empty.import(csv.as_bytes(), &mut |_, _| {}).unwrap();
    assert_eq!(res.report.format, "TCG Binder CSV");
    let rows = |cards: Vec<Card>| {
        let mut out: Vec<String> = cards
            .iter()
            .flat_map(|c| {
                c.purchase_rows().into_iter().map(move |r| {
                    let p = r.purchase;
                    format!(
                        "{}|{}|{}|{}|{}|{:?}|{}|{}|{}|{}",
                        c.name,
                        c.set.to_uppercase(),
                        c.collector_number,
                        c.language,
                        r.finish,
                        r.condition,
                        p.quantity,
                        p.price_paid,
                        p.acquired_date,
                        p.tags.join(";")
                    )
                })
            })
            .collect();
        out.sort();
        out
    };
    assert_eq!(rows(empty.store.cards()), rows(f.svc.store.cards()));
    assert_eq!(empty.store.tag_names(), ["binder A", "for sale"], "tags came back");
}

#[test]
fn valuation_report() {
    let f = fixture();
    let mut odd = f.svc.store.get(OPT).unwrap();
    odd.id = "zzzzzzzz-0000-4000-8000-000000000009".into();
    odd.name = "<script>alert('x')</script>".into();
    odd.set = "XYZ".into();
    odd.set_name = "<b>Set</b>".into();
    odd.released_at = String::new();
    f.svc.store.add_cards(vec![odd]).unwrap();

    let (name, html) = download(&f.api, "/api/games/mtg/export?format=report");
    assert!(name.contains("mtg-valuation-") && name.contains(".html"), "{name}");
    assert!(html.starts_with("<!DOCTYPE html>"));
    assert!(!html.contains("<script>") && !html.contains("<b>Set"), "card text is escaped");
    assert!(html.contains("&lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt;"));
    assert!(html.contains("Market value<b>$0.40</b>") && html.contains("Price paid<b>$0.50</b>"), "{html}");
    // Newest set first; per set a table with a total.
    let (m19, dom) = (html.find("Core Set 2019").unwrap(), html.find("Dominaria").unwrap());
    assert!(m19 < dom && dom < html.find("&lt;b&gt;Set&lt;/b&gt;").unwrap());
    assert!(html.contains("<td>Shock</td><td>156</td><td>Normal</td><td>Near Mint</td><td class=\"n\">2</td><td class=\"n\">$0.20</td><td class=\"n\">$0.40</td>"));
    assert!(html.contains("M19 total"));
    assert!(!html.contains("only the cards that matched"));

    let (_, red) = download(&f.api, "/api/games/mtg/export?format=report&f.Color=R");
    assert!(red.contains("only the cards that matched") && !red.contains("Dominaria"));

    assert_eq!(call(&f.api, "GET", "/api/games/mtg/export?format=pdf", None).0, 400);
    assert_eq!(call(&f.api, "GET", "/api/games/mtg/export?format=csv&sort=nope", None).0, 400);

    // Apps that save exports save these too.
    let out = tempfile::tempdir().unwrap();
    let api = Api::new("TCG Binder", "test", Client::Phone, Arc::new(SettingsStore::memory()), vec![f.svc.clone()])
        .with_export_dir(out.path().to_path_buf());
    let (status, saved) = call(&api, "POST", "/api/games/mtg/export?format=csv&set=m19", None);
    assert_eq!(status, 200, "{saved}");
    let path = saved["path"].as_str().unwrap();
    assert!(path.ends_with(".csv"));
    assert!(std::fs::read_to_string(path).unwrap().contains("Shock,M19"));
}

#[test]
fn import_with_tags() {
    let f = fixture();
    let p = preview(&f);
    let (status, v) = call(&f.api, "POST", "/api/games/mtg/import/commit", Some(json!({"token": p["token"], "tags": ["a;b"]})));
    assert_eq!(status, 400, "checked before the preview is used: {v}");
    let (status, v) = call(&f.api, "POST", "/api/games/mtg/import/commit", Some(json!({"token": p["token"], "tags": ["From echo "]})));
    assert_eq!(status, 200, "{v}");
    let t = get(&f.api, "/api/games/mtg/tags");
    assert_eq!(t["tags"][0]["name"], "From echo");
    assert_eq!(t["tags"][0]["copies"], v["cards"], "every imported copy, and only those");
    assert_eq!(t["untagged"]["copies"], 3, "the copies held before stay untagged");
}

#[test]
fn import_preview_then_commit() {
    let direct = fixture();
    let resp = direct.api.handle(&request("POST", "/api/games/mtg/import", ECHOMTG.to_vec()));
    assert_eq!(resp.status(), 200);
    let imported: Value = serde_json::from_slice(resp.body()).unwrap();

    let f = fixture();
    let (revision, cards) = (f.svc.store.revision(), f.svc.store.len());
    let p = preview(&f);
    assert_eq!(
        (p["format"].as_str(), p["rows"].as_u64(), p["copies"].as_i64(), p["cards"].as_u64()),
        (Some("EchoMTG"), Some(6), Some(4), Some(3))
    );
    assert_eq!((p["newCards"].as_u64(), p["alreadyOwned"].as_u64(), p["pricePaid"].as_f64()), (Some(2), Some(2), Some(1.95)));
    assert_eq!(p["header"].as_str(), std::str::from_utf8(ECHOMTG).unwrap().lines().next());
    let lines: Vec<u64> = p["unmatched"].as_array().unwrap().iter().map(|u| u["line"].as_u64().unwrap()).collect();
    assert_eq!(lines, [6, 7]);
    assert!(p["unmatched"][0]["text"].as_str().unwrap().contains("Soldier Token"));
    assert_eq!((f.svc.store.revision(), f.svc.store.len()), (revision, cards), "a preview saves nothing");

    assert_eq!(commit(&f, &json!("not the token")).0, 410);
    let (status, mut committed) = commit(&f, &p["token"]);
    assert_eq!(status, 200);
    let mut imported = imported;
    for v in [&mut committed, &mut imported] {
        let undo = v.as_object_mut().unwrap().remove("undo").unwrap();
        assert!(undo["backup"].as_str().unwrap().ends_with("-import.json"));
    }
    assert_eq!(committed, imported);
    let all = |f: &Fixture| serde_json::to_value(f.svc.store.cards()).unwrap();
    assert_eq!(all(&f), all(&direct));
    assert_eq!(commit(&f, &p["token"]).0, 410, "a preview is committed once");
}

#[test]
fn a_new_preview_replaces_the_old_one() {
    let f = fixture();
    let first = preview(&f);
    let second = preview(&f);
    assert_eq!(commit(&f, &first["token"]).0, 410);
    assert_eq!(commit(&f, &second["token"]).0, 200);
}

#[test]
fn paste_a_list() {
    let f = fixture();
    let list = "Deck\n4 Lightning Bolt (M11) 1\n2x Opt [DOM]\n1 Shock (M19) 156 *F*\n\nSideboard\n1 Not a Card (ZZZ) 9\n";
    let resp = f.api.handle(&request("POST", "/api/games/mtg/import?preview=1", list.as_bytes().to_vec()));
    assert_eq!(resp.status(), 200);
    let p: Value = serde_json::from_slice(resp.body()).unwrap();
    // Opt has no number, so it's matched by name (the fake's M19 printing).
    assert_eq!((p["format"].as_str(), p["rows"].as_u64(), p["copies"].as_i64()), (Some("Card list"), Some(4), Some(7)));
    let lines: Vec<u64> = p["unmatched"].as_array().unwrap().iter().map(|u| u["line"].as_u64().unwrap()).collect();
    assert_eq!(lines, [7]);
    assert_eq!(p["unmatched"][0]["text"], "1 Not a Card (ZZZ) 9");
    let (status, done) = commit(&f, &p["token"]);
    assert_eq!((status, done["cards"].as_i64()), (200, Some(7)));
    assert_eq!(f.svc.store.get("aaaaaaaa-0000-4000-8000-000000000001").unwrap().finish_copy_count("foil"), 1);
}

#[test]
fn undo_a_bulk_delete_and_restore_backups() {
    let f = fixture();
    let (status, r) = call(&f.api, "POST", "/api/games/mtg/bulk", Some(json!({ "keys": [SHOCK, OPT], "delete": true })));
    assert_eq!((status, r["Deleted"].as_u64()), (200, Some(2)));
    assert!(f.svc.store.is_empty());
    let undo = r["undo"].clone();
    assert!(undo["backup"].as_str().unwrap().ends_with("-bulk-delete.json"));

    let list = get(&f.api, "/api/games/mtg/backups");
    assert_eq!(
        (list[0]["name"].clone(), list[0]["reason"].as_str(), list[0]["cards"].as_u64()),
        (undo["backup"].clone(), Some("bulk-delete"), Some(2))
    );

    // Undo only while nothing else has changed.
    let stale = json!({ "name": undo["backup"], "ifRevision": undo["revision"].as_u64().unwrap() + 1 });
    assert_eq!(call(&f.api, "POST", "/api/games/mtg/backups/restore", Some(stale)).0, 409);
    let (status, restored) =
        call(&f.api, "POST", "/api/games/mtg/backups/restore", Some(json!({ "name": undo["backup"], "ifRevision": undo["revision"] })));
    assert_eq!((status, restored["cards"].as_u64()), (200, Some(2)));
    assert!(f.svc.store.get(SHOCK).is_some());
    assert_eq!(get(&f.api, "/api/games/mtg/backups")[0]["reason"], "restore", "a restore backs up first");

    for name in ["../collection.json", "collection.json", "x/../../collection.json"] {
        assert_eq!(call(&f.api, "POST", "/api/games/mtg/backups/restore", Some(json!({ "name": name }))).0, 404, "{name}");
    }
}

#[test]
fn jobs_report_progress() {
    let f = fixture();
    assert_eq!(get(&f.api, "/api/games/mtg/jobs")["jobs"].as_array().unwrap().len(), 0);
    let seen = std::cell::RefCell::new(Value::Null);
    let res = f.api.job(&f.svc, "import", |r| {
        r.report(3, 10, "Matching cards");
        let Reply::Json(v) = f.api.jobs(&f.svc) else { unreachable!() };
        *seen.borrow_mut() = v;
        // The same job can't start twice.
        assert!(f.api.job(&f.svc, "import", |_| Ok(Value::Null)).is_err());
        Ok(json!({}))
    });
    assert!(res.is_ok());
    let jobs = seen.into_inner();
    assert_eq!(jobs["jobs"][0], json!({ "name": "import", "done": 3, "total": 10, "message": "Matching cards" }));
    assert_eq!(get(&f.api, "/api/games/mtg/jobs")["jobs"].as_array().unwrap().len(), 0, "gone once finished");
}

/// A filtered CSV export holds only the copies the filters pick, as the table shows
/// them, not every copy of each card matched.
#[test]
fn a_filtered_export_keeps_the_filtered_copies() {
    let f = fixture();
    let (_, lot) = call(&f.api, "POST", "/api/games/mtg/lots", Some(json!({ "name": "Box", "total_paid": 1.0, "split": "even" })));
    let id = lot["id"].as_str().unwrap();
    call(&f.api, "POST", &format!("/api/games/mtg/lots/cards?id={id}"), Some(json!({"cards": [{"key": SHOCK, "quantity": 1}]})));
    let rows = |path: &str| -> Vec<(String, String)> {
        let (_, body) = download(&f.api, path);
        // The fixture's fields have no commas or quotes, so a plain split reads them.
        let mut lines = body.lines().map(|l| l.split(',').map(str::to_string).collect::<Vec<_>>());
        let h = lines.next().unwrap();
        let (name, qty) = (h.iter().position(|c| c == "Name").unwrap(), h.iter().position(|c| c == "Quantity").unwrap());
        lines.map(|x| (x[name].clone(), x[qty].clone())).collect()
    };
    let s = |v: &[(&str, &str)]| v.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect::<Vec<_>>();
    assert_eq!(rows(&format!("/api/games/mtg/export?format=csv&lot={id}")), s(&[("Shock", "1")]), "only the copy in the lot");
    assert_eq!(rows("/api/games/mtg/export?format=csv&lot=none"), s(&[("Opt", "1"), ("Shock", "1")]), "only the copies in no lot");
    assert_eq!(rows("/api/games/mtg/export?format=csv&finish=foil"), s(&[("Opt", "1")]));
    assert_eq!(rows("/api/games/mtg/export?format=csv"), s(&[("Opt", "1"), ("Shock", "1"), ("Shock", "1")]), "unfiltered: every copy");
}
