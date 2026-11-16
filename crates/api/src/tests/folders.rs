//! The data folder: moving it, a fixed one, and conflict copies.

use super::*;

/// An app whose local folder is also its data folder (the default), holding one card,
/// and a second folder `target` to move it to.
struct FolderFixture {
    api: Api,
    svc: Arc<GameService>,
    settings: Arc<SettingsStore>,
    local: std::path::PathBuf,
    target: std::path::PathBuf,
    _dir: TempDir,
    _scryfall: FakeServer,
}

fn folder_fixture(fixed: bool) -> FolderFixture {
    let (scryfall, fake) = testing::search_server();
    let dir = tempfile::tempdir().unwrap();
    let (local, target) = (dir.path().join("local"), dir.path().join("Cloud Drive/Cards"));
    let g = Magic::with_clients(scryfall, mtgjson::Client::new(&fake.url));
    let svc = Arc::new(GameService::open(&local, &local, Box::new(g)).unwrap());
    let card = Card { id: SHOCK.into(), name: "Shock".into(), set: "M19".into(), language: "en".into(), ..Card::default() };
    svc.store.add_cards(vec![held(card, "nonfoil", Condition::NearMint, Purchase { quantity: 2, ..Default::default() })]).unwrap();
    let settings = Arc::new(SettingsStore::open(&local));
    let folders = Folders { local: local.clone(), data: local.clone(), fixed, first_run: false };
    let api = Api::new("TCG Binder", "test", Client::Web, settings.clone(), vec![svc.clone()]).with_folders(folders);
    FolderFixture { api, svc, settings, local, target, _dir: dir, _scryfall: fake }
}

#[test]
fn move_the_data_folder() {
    let f = folder_fixture(false);
    let info = get(&f.api, "/api/data-folder");
    assert_eq!((info["isDefault"].as_bool(), info["fixed"].as_bool(), info["canPick"].as_bool()), (Some(true), Some(false), Some(false)));
    assert_eq!(info["path"].as_str(), f.local.to_str());
    assert_eq!(call(&f.api, "POST", "/api/data-folder/pick", None).0, 501);

    let target = f.target.display().to_string();
    let (status, seen) = call(&f.api, "POST", "/api/data-folder/inspect", Some(json!({ "path": target })));
    assert_eq!(
        (status, seen["exists"].as_bool(), seen["writable"].as_bool(), seen["games"].as_array().map(Vec::len)),
        (200, Some(false), Some(true), Some(0))
    );
    assert_eq!(call(&f.api, "POST", "/api/data-folder/inspect", Some(json!({ "path": "relative" }))).0, 400);
    assert_eq!(call(&f.api, "POST", "/api/data-folder", Some(json!({ "path": target, "mode": "sideways" }))).0, 400);
    assert_eq!(call(&f.api, "PUT", "/api/settings", Some(json!({ "data_dir": target }))).0, 400, "not through settings");

    let (status, moved) = call(&f.api, "POST", "/api/data-folder", Some(json!({ "path": target, "mode": "move" })));
    assert_eq!((status, moved["restart"].as_bool(), moved["restartsItself"].as_bool()), (200, Some(true), Some(false)));
    assert_eq!(f.settings.get().data_dir, target);
    assert!(!f.local.join("mtg/collection.json").exists());
    let found = tcgbinder_core::datafolder::inspect(&f.target, &["mtg"]);
    assert_eq!(found.games[0].cards, 1);
    assert!(f.target.join("mtg/backups").read_dir().unwrap().any(|e| e.unwrap().file_name().to_string_lossy().ends_with("-move.json")));
    // Until the app restarts, nothing may be saved (the open collection has moved).
    assert_eq!(call(&f.api, "DELETE", &format!("/api/games/mtg/card?key={SHOCK}"), None).0, 409);
}

#[test]
fn a_fixed_data_folder_stays() {
    let f = folder_fixture(true);
    let (status, _) = call(&f.api, "POST", "/api/data-folder", Some(json!({ "path": f.target.display().to_string(), "mode": "move" })));
    assert_eq!(status, 409);
    assert!(f.local.join("mtg/collection.json").exists());
}

#[test]
fn merge_or_discard_conflict_copies() {
    let f = folder_fixture(false);
    let game_dir = f.local.join("mtg");
    // Another device saved at the same time: Shock ×3 and a card this one lacks.
    let theirs = json!({ "game": "mtg", "cards": {
        SHOCK: { "id": SHOCK, "name": "Shock", "lang": "en", "copies": { "nonfoil": { "NM": [{ "quantity": 3 }] } } },
        OPT: { "id": OPT, "name": "Opt", "lang": "en", "copies": { "foil": { "LP": [{ "quantity": 1 }] } } },
    }});
    std::fs::write(game_dir.join("collection 2.json"), theirs.to_string()).unwrap();
    std::fs::write(game_dir.join("collection (conflicted copy).json"), theirs.to_string()).unwrap();
    let conflicts = get(&f.api, "/api/data-folder")["conflicts"].clone();
    assert_eq!(conflicts.as_array().unwrap().len(), 2);
    assert_eq!((conflicts[1]["name"].as_str(), conflicts[1]["cards"].as_u64()), (Some("collection 2.json"), Some(2)));

    let (status, merged) = call(&f.api, "POST", "/api/games/mtg/conflicts/merge", Some(json!({ "name": "collection 2.json" })));
    assert_eq!((status, merged["changed"].as_u64()), (200, Some(2)));
    assert_eq!(f.svc.store.get(SHOCK).unwrap().copy_count(), 3, "the larger quantity, not 2 + 3");
    assert!(f.svc.store.get(OPT).is_some());
    assert!(!game_dir.join("collection 2.json").exists());
    assert!(merged["undo"]["backup"].as_str().unwrap().ends_with("-merge.json"));

    let (status, _) =
        call(&f.api, "POST", "/api/games/mtg/conflicts/discard", Some(json!({ "name": "collection (conflicted copy).json" })));
    assert_eq!(status, 200);
    assert!(get(&f.api, "/api/data-folder")["conflicts"].as_array().unwrap().is_empty());
    for name in ["collection.json", "../local/settings.json", "price-history.json"] {
        assert_eq!(call(&f.api, "POST", "/api/games/mtg/conflicts/discard", Some(json!({ "name": name }))).0, 404, "{name}");
    }
    assert!(game_dir.join("collection.json").exists());
}

#[test]
fn merge_a_records_conflict_copy() {
    let f = folder_fixture(false);
    let game_dir = f.local.join("mtg");
    let theirs = json!({ "records": [{ "id": "theirs", "name": "Their deck", "created": "2026-01-01", "entries": [] }] });
    std::fs::write(game_dir.join("lists 2.json"), theirs.to_string()).unwrap();
    let conflicts = get(&f.api, "/api/data-folder")["conflicts"].clone();
    assert_eq!((conflicts[0]["name"].as_str(), conflicts[0]["kind"].as_str()), (Some("lists 2.json"), Some("lists")));
    let (status, merged) = call(&f.api, "POST", "/api/games/mtg/conflicts/merge", Some(json!({ "name": "lists 2.json" })));
    assert_eq!((status, merged["changed"].as_u64()), (200, Some(1)));
    assert_eq!(f.svc.lists.get("theirs").unwrap().name, "Their deck");
    assert!(!game_dir.join("lists 2.json").exists());
}
