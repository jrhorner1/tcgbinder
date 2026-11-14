//! The data folder: where it is, choosing another (with the platform's folder picker
//! where there is one), and the conflict copies sync services leave.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use http::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};

use tcgbinder_core::collection::read_export;
use tcgbinder_core::datafolder::{self, Mode};
use tcgbinder_core::lots::LotsData;
use tcgbinder_core::service::GameService;

use crate::{Api, Error, Reply, Result, bad, decode, internal, not_found, undo};

/// Where the app keeps its data (see `tcgbinder_core::datadir`).
#[derive(Clone, Debug)]
pub struct Folders {
    /// This device's folder: settings, log, caches. Also the default data folder.
    pub local: PathBuf,
    /// The folder holding the collections now.
    pub data: PathBuf,
    /// Set by `--data-dir` or `TCGBINDER_DATA_DIR`, so Settings can't change it.
    pub fixed: bool,
    /// Nothing was there when the app started: no collection and no settings.
    pub first_run: bool,
}

/// Shows the system's folder picker and returns the folder chosen, `None` if cancelled.
pub type PickFolder = Arc<dyn Fn() -> Option<PathBuf> + Send + Sync>;

/// Restarts the app, so it opens the new data folder.
pub type Restart = Arc<dyn Fn() + Send + Sync>;

#[derive(Deserialize)]
struct PathReq {
    path: String,
}

#[derive(Deserialize)]
struct NameReq {
    name: String,
}

/// The game's folder in the data folder.
fn game_dir(g: &GameService) -> &Path {
    g.store.file_path().parent().unwrap_or(Path::new("."))
}

impl Api {
    /// Says where the data folder is and how it can be changed here.
    pub fn with_folders(mut self, folders: Folders) -> Api {
        self.folders = Some(folders);
        self
    }

    /// Turns on "Change…" with the system's folder picker.
    pub fn with_folder_picker(mut self, pick: PickFolder) -> Api {
        self.pick_folder = Some(pick);
        self
    }

    /// Lets the app restart itself after the data folder changes.
    pub fn with_restart(mut self, restart: Restart) -> Api {
        self.restart = Some(restart);
        self
    }

    fn folders(&self) -> Result<&Folders> {
        self.folders.as_ref().ok_or_else(|| not_found("the data folder can't be changed here"))
    }

    fn game_ids(&self) -> Vec<&'static str> {
        self.games.iter().map(|g| g.game.id()).collect()
    }

    /// The data folder, whether it can be changed, and any conflict copies.
    pub(crate) fn data_folder(&self) -> Result {
        let f = self.folders()?;
        let conflicts: Vec<Value> = self
            .games
            .iter()
            .flat_map(|g| {
                datafolder::conflict_copies(game_dir(g)).into_iter().map(move |name| {
                    let kind = if name.starts_with("lots") { "lots" } else { "collection" };
                    let cards = std::fs::read(game_dir(g).join(&name)).ok().and_then(|b| read_export(&b).ok()).map(|(_, c)| c.len());
                    json!({ "game": g.game.id(), "name": name, "kind": kind, "cards": cards })
                })
            })
            .collect();
        Ok(Reply::Json(json!({
            "path": f.data.display().to_string(), "isDefault": f.data == f.local, "fixed": f.fixed,
            "defaultPath": f.local.display().to_string(), "firstRun": f.first_run, "conflicts": conflicts,
            "canPick": self.pick_folder.is_some(), "restartsItself": self.restart.is_some(),
        })))
    }

    /// Shows the folder picker.
    pub(crate) fn pick_data_folder(&self) -> Result {
        let pick = self.pick_folder.as_ref().ok_or_else(|| Error {
            status: StatusCode::NOT_IMPLEMENTED,
            message: "there is no folder picker here; type the folder's path instead".into(),
        })?;
        Ok(Reply::Json(json!({ "path": pick().map(|p| p.display().to_string()) })))
    }

    /// What a folder holds.
    pub(crate) fn inspect_data_folder(&self, body: &[u8]) -> Result {
        let req: PathReq = decode(body)?;
        let path = PathBuf::from(req.path.trim());
        if !path.is_absolute() {
            return Err(bad("choose a full path to a folder, such as /Users/you/Documents/Cards"));
        }
        Ok(Reply::Json(json!(datafolder::inspect(&path, &self.game_ids()))))
    }

    /// Switches to another data folder (`"use"` what is there, or `"move"` or `"copy"`
    /// the current data there), saves the choice and asks for a restart. Until then,
    /// changes are refused, since the open collection may have moved.
    pub(crate) fn change_data_folder(&self, body: &[u8]) -> Result {
        #[derive(Deserialize)]
        struct Req {
            path: String,
            mode: String,
        }
        let req: Req = decode(body)?;
        let f = self.folders()?;
        if f.fixed {
            return Err(Error {
                status: StatusCode::CONFLICT,
                message: "the data folder is set by --data-dir or TCGBINDER_DATA_DIR; change it there".into(),
            });
        }
        let mode = Mode::parse(&req.mode).ok_or_else(|| bad("mode must be use, move or copy"))?;
        let path = req.path.trim();
        let target = if path.is_empty() { f.local.clone() } else { PathBuf::from(path) };
        let target = datafolder::check_target(&f.data, &target).map_err(|e| bad(e.to_string()))?;
        if mode == Mode::Move {
            for g in &self.games {
                g.store.backup("move").map_err(internal)?;
            }
        }
        datafolder::transfer(&f.data, &target, &self.game_ids(), mode).map_err(|e| bad(format!("{e:#}")))?;
        let mut s = self.settings.get();
        s.data_dir = if target == f.local { String::new() } else { target.display().to_string() };
        self.settings.set(s).map_err(internal)?;
        self.switched.store(true, std::sync::atomic::Ordering::SeqCst);
        tracing::info!(to = %target.display(), mode = req.mode, "data folder chosen; restart needed");
        if let Some(restart) = self.restart.clone() {
            // After this answer has reached the page.
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(500));
                restart();
            });
        }
        Ok(Reply::Json(json!({ "restart": true, "restartsItself": self.restart.is_some(), "path": target.display().to_string() })))
    }

    /// A conflict copy in the game's folder, checked against the listed ones.
    fn conflict_path(g: &GameService, name: &str) -> Result<PathBuf> {
        if !datafolder::conflict_copies(game_dir(g)).iter().any(|n| n == name) {
            return Err(not_found(format!("there is no conflict copy named {name:?}")));
        }
        Ok(game_dir(g).join(name))
    }

    /// Merges a conflict copy into the collection (after a backup) or the lots, then
    /// deletes it.
    pub(crate) fn merge_conflict(&self, g: &GameService, body: &[u8]) -> Result {
        let req: NameReq = decode(body)?;
        let path = Self::conflict_path(g, &req.name)?;
        let data = std::fs::read(&path).map_err(|e| internal(e.into()))?;
        if req.name.starts_with("lots") {
            let theirs: LotsData = serde_json::from_slice(&data).map_err(|e| bad(format!("{} can't be read: {e}", req.name)))?;
            let changed = g.lots.merge_copy(theirs).map_err(internal)?;
            std::fs::remove_file(&path).map_err(|e| internal(e.into()))?;
            tracing::info!(game = g.game.id(), copy = req.name, changed, "lots conflict copy merged");
            return Ok(Reply::Json(json!({ "changed": changed })));
        }
        let (game, cards) = read_export(&data).map_err(|e| bad(format!("{} can't be read: {e:#}", req.name)))?;
        if !game.is_empty() && game != g.game.id() {
            return Err(bad(format!("{} is a {game:?} collection", req.name)));
        }
        let backup = g.store.backup("merge").map_err(internal)?;
        let changed = g.store.merge_copy(cards).map_err(internal)?;
        std::fs::remove_file(&path).map_err(|e| internal(e.into()))?;
        tracing::info!(game = g.game.id(), copy = req.name, changed, "conflict copy merged");
        Ok(Reply::Json(json!({ "changed": changed, "undo": undo(g, &backup) })))
    }

    /// Deletes a conflict copy without merging it.
    pub(crate) fn discard_conflict(&self, g: &GameService, body: &[u8]) -> Result {
        let req: NameReq = decode(body)?;
        let path = Self::conflict_path(g, &req.name)?;
        std::fs::remove_file(&path).map_err(|e| internal(e.into()))?;
        tracing::info!(game = g.game.id(), copy = req.name, "conflict copy discarded");
        Ok(Reply::Json(json!({ "discarded": req.name })))
    }
}
