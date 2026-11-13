//! The app's data folder: one subfolder per game, so each game keeps its own
//! collection, price history and caches.
//!
//! ```text
//! <data>/settings.json
//! <data>/tcgbinder.lock
//! <data>/tcgbinder.log
//! <data>/mtg/collection.json
//! <data>/mtg/price-history.json
//! <data>/mtg/scryfall-sets.json
//! ```

use std::path::{Path, PathBuf};

/// The app's identifier: the bundle ID on Apple platforms, and the data folder's name.
pub const APP_ID: &str = "io.github.jrhorner1.tcgbinder";

/// The name of each game's collection file.
pub const COLLECTION_FILE: &str = "collection.json";

/// The folder for a game's files, created if needed.
pub fn game_dir(root: &Path, game_id: &str) -> std::io::Result<PathBuf> {
    let dir = root.join(game_id);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// The desktop data folder, the same one the Tauri app uses for its app data:
///
/// ```text
/// macOS    ~/Library/Application Support/io.github.jrhorner1.tcgbinder
/// Windows  %APPDATA%\io.github.jrhorner1.tcgbinder
/// Linux    $XDG_DATA_HOME/io.github.jrhorner1.tcgbinder (usually ~/.local/share/...)
/// ```
pub fn default_root() -> Option<PathBuf> {
    let base = if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    };
    base.map(|b| b.join(APP_ID))
}
