//! Choosing where the collection lives: what a folder holds, taking the data along to
//! a new folder, and the conflict copies sync services leave when two devices save at
//! once. See `datadir` for the two folders.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::collection::read_export;
use crate::datadir::{COLLECTION_FILE, PRICE_HISTORY_FILE};
use crate::lists::LISTS_FILE;
use crate::lots::LOTS_FILE;
use crate::sales::SALES_FILE;
use crate::watchlist::WATCHLIST_FILE;

/// The folder of backups in each game's data folder.
const BACKUPS: &str = "backups";

/// A game's collection found in a folder.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FoundCollection {
    pub id: String,
    /// Cards (printings) in it; 0 when it can't be read.
    pub cards: usize,
}

/// What a folder holds, before it is chosen.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Inspection {
    pub exists: bool,
    /// Whether the app can write there (for a folder that doesn't exist yet: whether
    /// it can be created).
    pub writable: bool,
    /// The games with a collection there.
    pub games: Vec<FoundCollection>,
}

/// How to switch to a new data folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Use what is there (or start fresh there); the current data stays where it is.
    Use,
    /// Take the current data along, then delete it from the old folder.
    Move,
    /// Take a copy of the current data along.
    Copy,
}

impl Mode {
    pub fn parse(s: &str) -> Option<Mode> {
        match s {
            "use" => Some(Mode::Use),
            "move" => Some(Mode::Move),
            "copy" => Some(Mode::Copy),
            _ => None,
        }
    }
}

/// Whether a file can be created in `dir`.
fn can_write(dir: &Path) -> bool {
    tempfile::tempfile_in(dir).is_ok()
}

/// What `path` holds for each of the games.
pub fn inspect(path: &Path, game_ids: &[&str]) -> Inspection {
    let exists = path.is_dir();
    let writable = if exists { can_write(path) } else { path.ancestors().skip(1).find(|p| p.is_dir()).is_some_and(can_write) };
    let games = game_ids
        .iter()
        .filter_map(|id| {
            let file = path.join(id).join(COLLECTION_FILE);
            file.is_file().then(|| FoundCollection {
                id: id.to_string(),
                cards: fs::read(&file).ok().and_then(|b| read_export(&b).ok()).map_or(0, |(_, cards)| cards.len()),
            })
        })
        .collect();
    Inspection { exists, writable, games }
}

/// Checks that `to` can become the data folder in place of `from`, and returns it
/// as an absolute path.
pub fn check_target(from: &Path, to: &Path) -> Result<PathBuf> {
    if !to.is_absolute() {
        bail!("choose a full path to a folder, such as /Users/you/Documents/Cards");
    }
    let same = |a: &Path, b: &Path| a == b || fs::canonicalize(a).ok().zip(fs::canonicalize(b).ok()).is_some_and(|(a, b)| a == b);
    if same(from, to) {
        bail!("that is already the data folder");
    }
    if to.exists() && !to.is_dir() {
        bail!("{} is a file, not a folder", to.display());
    }
    Ok(to.to_path_buf())
}

/// Switches the data from `from` to `to` for the games: with [`Mode::Move`] or
/// [`Mode::Copy`], each game's collection, price history, lots, sales, lists, watchlist and backups are copied and
/// checked (and, for a move, then deleted from `from`). Refused if `to` already holds
/// a collection for any of them.
pub fn transfer(from: &Path, to: &Path, game_ids: &[&str], mode: Mode) -> Result<()> {
    fs::create_dir_all(to).with_context(|| format!("creating {}", to.display()))?;
    if !can_write(to) {
        bail!("can't write to {}", to.display());
    }
    if mode == Mode::Use {
        return Ok(());
    }
    if let Some(found) = inspect(to, game_ids).games.first() {
        bail!("{} already has a {} collection ({} cards); use it instead, or choose an empty folder", to.display(), found.id, found.cards);
    }
    let mut copied = Vec::new();
    for id in game_ids {
        let (src, dst) = (from.join(id), to.join(id));
        if !src.join(COLLECTION_FILE).is_file() {
            continue;
        }
        fs::create_dir_all(&dst)?;
        for name in [COLLECTION_FILE, PRICE_HISTORY_FILE, LOTS_FILE, SALES_FILE, LISTS_FILE, WATCHLIST_FILE] {
            if src.join(name).is_file() {
                copy_checked(&src.join(name), &dst.join(name))?;
                copied.push(src.join(name));
            }
        }
        if let Ok(entries) = fs::read_dir(src.join(BACKUPS)) {
            fs::create_dir_all(dst.join(BACKUPS))?;
            for e in entries.flatten().filter(|e| e.path().is_file()) {
                copy_checked(&e.path(), &dst.join(BACKUPS).join(e.file_name()))?;
                copied.push(e.path());
            }
        }
    }
    if mode == Mode::Move {
        for path in &copied {
            if let Err(e) = fs::remove_file(path) {
                tracing::warn!(path = %path.display(), error = %e, "moved, but could not delete the original");
            }
        }
        for id in game_ids {
            let _ = fs::remove_dir(from.join(id).join(BACKUPS)); // only if now empty
        }
    }
    tracing::info!(from = %from.display(), to = %to.display(), ?mode, files = copied.len(), "data folder changed");
    Ok(())
}

/// Copies a JSON file and checks the copy reads back as JSON.
fn copy_checked(src: &Path, dst: &Path) -> Result<()> {
    fs::copy(src, dst).with_context(|| format!("copying {} to {}", src.display(), dst.display()))?;
    let bytes = fs::read(dst).with_context(|| format!("reading back {}", dst.display()))?;
    serde_json::from_slice::<serde::de::IgnoredAny>(&bytes).with_context(|| format!("the copy {} doesn't read back", dst.display()))?;
    Ok(())
}

/// The conflict copies in a game's data folder: files named like `collection*.json`,
/// `lots*.json`, `sales*.json`, `lists*.json` or `watchlist*.json` other than those
/// files themselves, as sync services name the
/// version they didn't keep ("collection 2.json", "collection (conflicted copy).json",
/// "lots-LAPTOP.json", "collection (1).json"). Sorted by name.
pub fn conflict_copies(game_dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(game_dir) else { return Vec::new() };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| {
            n.ends_with(".json")
                && [
                    ("collection", COLLECTION_FILE),
                    ("lots", LOTS_FILE),
                    ("sales", SALES_FILE),
                    ("lists", LISTS_FILE),
                    ("watchlist", WATCHLIST_FILE),
                ]
                .iter()
                .any(|(stem, file)| n.starts_with(stem) && n != file)
        })
        .collect();
    names.sort();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collection(dir: &Path, game: &str, cards: usize) {
        let cards: serde_json::Map<String, serde_json::Value> = (0..cards)
            .map(|i| (format!("c{i}"), serde_json::json!({"id": format!("c{i}"), "copies": {"nonfoil": {"NM": [{"quantity": 1}]}}})))
            .collect();
        fs::create_dir_all(dir.join(game)).unwrap();
        fs::write(dir.join(game).join(COLLECTION_FILE), serde_json::json!({"game": game, "cards": cards}).to_string()).unwrap();
    }

    #[test]
    fn inspect_folders() {
        let dir = tempfile::tempdir().unwrap();
        let empty = inspect(&dir.path().join("new/inner"), &["mtg"]);
        assert_eq!((empty.exists, empty.writable, empty.games.len()), (false, true, 0));
        collection(dir.path(), "mtg", 3);
        let found = inspect(dir.path(), &["mtg", "other"]);
        assert_eq!((found.exists, found.writable), (true, true));
        assert_eq!(found.games, [FoundCollection { id: "mtg".into(), cards: 3 }]);
    }

    #[test]
    fn move_copy_and_use() {
        let dir = tempfile::tempdir().unwrap();
        let (from, to) = (dir.path().join("from"), dir.path().join("to"));
        collection(&from, "mtg", 2);
        fs::write(from.join("mtg").join(PRICE_HISTORY_FILE), "{}").unwrap();
        fs::write(from.join("mtg").join(SALES_FILE), "{}").unwrap();
        fs::create_dir_all(from.join("mtg/backups")).unwrap();
        fs::write(from.join("mtg/backups/collection-20261114-120000-move.json"), "{}").unwrap();
        fs::write(from.join("mtg/scryfall-sets.json"), "{}").unwrap(); // a cache: stays

        transfer(&from, &to, &["mtg"], Mode::Copy).unwrap();
        assert_eq!(inspect(&to, &["mtg"]).games[0].cards, 2);
        assert!(from.join("mtg").join(COLLECTION_FILE).exists());
        assert!(transfer(&from, &to, &["mtg"], Mode::Move).unwrap_err().to_string().contains("already has"));

        let moved = dir.path().join("moved");
        transfer(&from, &moved, &["mtg"], Mode::Move).unwrap();
        assert!(moved.join("mtg").join(SALES_FILE).exists() && !from.join("mtg").join(SALES_FILE).exists());
        assert!(moved.join("mtg/price-history.json").exists() && moved.join("mtg/backups/collection-20261114-120000-move.json").exists());
        assert!(!from.join("mtg").join(COLLECTION_FILE).exists() && !from.join("mtg/backups").exists());
        assert!(from.join("mtg/scryfall-sets.json").exists());

        transfer(&from, &to, &["mtg"], Mode::Use).unwrap(); // adopts what is there
        assert!(check_target(&to, &to).is_err());
        assert!(check_target(&to, Path::new("relative/path")).is_err());
        assert!(check_target(&to, &moved.join("mtg/price-history.json")).is_err());
    }

    #[test]
    fn finds_conflict_copies() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "collection.json",
            "collection 2.json",
            "collection (conflicted copy 2026-11-14).json",
            "collection_abc.tmp",
            "price-history.json",
            "lots.json",
            "lots 2.json",
            "sales.json",
            "sales (conflicted copy).json",
            "lists.json",
            "lists-LAPTOP.json",
            "watchlist.json",
            "watchlist 2.json",
        ] {
            fs::write(dir.path().join(name), "{}").unwrap();
        }
        assert_eq!(
            conflict_copies(dir.path()),
            [
                "collection (conflicted copy 2026-11-14).json",
                "collection 2.json",
                "lists-LAPTOP.json",
                "lots 2.json",
                "sales (conflicted copy).json",
                "watchlist 2.json"
            ]
        );
        assert!(conflict_copies(&dir.path().join("missing")).is_empty());
    }
}
