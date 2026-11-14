//! The app's two folders, each with one subfolder per game.
//!
//! The **local folder** is always the platform's app data folder. It holds what
//! belongs to this device: settings, the log, the instance locks and caches.
//!
//! ```text
//! <local>/settings.json
//! <local>/tcgbinder.log
//! <local>/locks/<hash of the data folder>.lock
//! <local>/mtg/scryfall-sets.json
//! ```
//!
//! The **data folder** holds the collection. It is the local folder unless the user
//! chose another (for example one a sync service shares between devices).
//!
//! ```text
//! <data>/mtg/collection.json
//! <data>/mtg/price-history.json
//! <data>/mtg/backups/
//! ```

use std::path::{Path, PathBuf};

/// The app's identifier: the bundle ID on Apple platforms, and the data folder's name.
pub const APP_ID: &str = "io.github.jrhorner1.tcgbinder";

/// The name of each game's collection file.
pub const COLLECTION_FILE: &str = "collection.json";

/// The name of each game's price history file.
pub const PRICE_HISTORY_FILE: &str = "price-history.json";

/// The single-instance lock for a data folder, in the local folder: named after the
/// data folder, so two different data folders can be open at once.
pub fn lock_path(local: &Path, data: &Path) -> PathBuf {
    // FNV-1a: stable across builds, unlike the standard library's hasher.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in data.as_os_str().as_encoded_bytes() {
        hash = (hash ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3);
    }
    local.join("locks").join(format!("{hash:016x}.lock"))
}

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
