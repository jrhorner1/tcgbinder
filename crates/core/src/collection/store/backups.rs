//! Backups of the collection file: taken before an import, a bulk edit or delete, a
//! restore, a merge, a move, a tag rename or delete or a lot deleted, kept in
//! `backups/` beside it, listed, and restored. A backup may carry more than the
//! collection (the game's lots, see `GameService::backup`): the extra keys are kept in
//! the file and ignored here.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::Serialize;

use super::{Store, StoreData};

/// How many backups of the collection are kept.
pub const BACKUPS_KEPT: usize = 10;

/// Why a backup was taken, as the last part of its file name.
pub const BACKUP_REASONS: &[&str] =
    &["import", "bulk-edit", "bulk-delete", "restore", "merge", "move", "tag-rename", "tag-delete", "delete-all", "lot-delete"];

/// A backup of the collection file, as listed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BackupInfo {
    /// The file name, which identifies the backup.
    pub name: String,
    /// When it was taken, `YYYY-MM-DD HH:MM:SS` (local time).
    pub date: String,
    /// Why it was taken (one of [`BACKUP_REASONS`]).
    pub reason: String,
    /// Cards (printings) in it.
    pub cards: usize,
    /// Its size in bytes.
    pub size: u64,
    /// Lots in it; None for a backup taken before lots were kept in backups.
    pub lots: Option<usize>,
}

/// Reads a backup's name: `collection-YYYYMMDD-HHMMSS-<reason>.json`, possibly with
/// `-N` after the time when more were taken in the same second. Returns the date, N
/// (1 without it) and the reason.
fn parse_backup_name(name: &str) -> Option<(String, u32, String)> {
    let rest = name.strip_prefix("collection-")?.strip_suffix(".json")?;
    let (day, rest) = rest.split_once('-')?;
    let (time, rest) = rest.split_once('-')?;
    let (seq, reason) = match rest.split_once('-') {
        Some((n, reason)) if n.bytes().all(|b| b.is_ascii_digit()) => (n.parse().ok()?, reason),
        _ => (1, rest),
    };
    let stamp = chrono::NaiveDateTime::parse_from_str(&format!("{day}{time}"), "%Y%m%d%H%M%S").ok()?;
    BACKUP_REASONS.contains(&reason).then(|| (stamp.format("%Y-%m-%d %H:%M:%S").to_string(), seq, reason.to_string()))
}

impl Store {
    /// The folder backups are kept in: `backups` next to the collection file.
    pub fn backup_dir(&self) -> PathBuf {
        self.file_path().with_file_name("backups")
    }

    /// Copies the collection file to the backups folder as
    /// `collection-YYYYMMDD-HHMMSS-<reason>.json` and keeps the [`BACKUPS_KEPT`] newest.
    /// Returns the backup's name. (`GameService::backup` adds the game's lots.)
    pub fn backup(&self, reason: &str) -> Result<String> {
        let bytes = fs::read(self.file_path()).context("reading the collection to back it up")?;
        self.backup_with(reason, &bytes)
    }

    /// Saves `bytes` (the collection file, perhaps with more in it) as a backup, as
    /// [`Store::backup`] does.
    pub fn backup_with(&self, reason: &str, bytes: &[u8]) -> Result<String> {
        if !BACKUP_REASONS.contains(&reason) {
            bail!("unknown backup reason {reason:?}");
        }
        let dir = self.backup_dir();
        fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
        // Backups taken in the same second are numbered in order: -2-, -3-, ...
        let prefix = format!("collection-{stamp}-");
        let last = self
            .backup_names()?
            .iter()
            .filter(|n| n.starts_with(&prefix))
            .filter_map(|n| parse_backup_name(n))
            .map(|(_, seq, _)| seq)
            .max();
        let name = match last {
            None => format!("{prefix}{reason}.json"),
            Some(seq) => format!("{prefix}{}-{reason}.json", seq + 1),
        };
        crate::jsonfile::write_atomic(&dir.join(&name), "backup_", bytes).context("saving a backup")?;
        let mut names = self.backup_names()?;
        while names.len() > BACKUPS_KEPT {
            let old = names.remove(0);
            if let Err(e) = fs::remove_file(dir.join(&old)) {
                tracing::warn!(backup = old, error = %e, "could not delete an old backup");
            }
        }
        tracing::info!(backup = name, "collection backed up");
        Ok(name)
    }

    /// The backups' file names, oldest first.
    fn backup_names(&self) -> Result<Vec<String>> {
        let entries = match fs::read_dir(self.backup_dir()) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e).context("listing backups"),
        };
        let mut names: Vec<(String, u32, String)> = entries
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter_map(|n| parse_backup_name(&n).map(|(date, seq, _)| (date, seq, n)))
            .collect();
        names.sort();
        Ok(names.into_iter().map(|(_, _, n)| n).collect())
    }

    /// Deletes every backup (deleting all data without keeping one).
    pub fn delete_backups(&self) -> Result<usize> {
        let dir = self.backup_dir();
        let names = self.backup_names()?;
        for n in &names {
            fs::remove_file(dir.join(n)).with_context(|| format!("deleting the backup {n}"))?;
        }
        Ok(names.len())
    }

    /// The backups, newest first.
    pub fn backups(&self) -> Result<Vec<BackupInfo>> {
        let dir = self.backup_dir();
        let mut out = Vec::new();
        for name in self.backup_names()?.into_iter().rev() {
            let Some((date, _, reason)) = parse_backup_name(&name) else { continue };
            let path = dir.join(&name);
            let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            let bytes = fs::read(&path).unwrap_or_default();
            let cards = serde_json::from_slice::<StoreData>(&bytes).map_or(0, |d| d.cards.len());
            let lots = serde_json::from_slice::<serde_json::Value>(&bytes).ok().and_then(|v| v.get("lots")?.as_array().map(Vec::len));
            out.push(BackupInfo { name, date, reason, cards, size, lots });
        }
        Ok(out)
    }

    /// Replaces the collection with a backup's, after backing up the current one
    /// (reason `restore`). `name` must be one of the listed backups. Returns the name
    /// of the backup taken first.
    pub fn restore(&self, name: &str) -> Result<String> {
        let bytes = self.backup_bytes(name)?;
        self.restore_from(&bytes, || self.backup("restore"))
    }

    /// A backup's file, by its name (one of the listed backups).
    pub fn backup_bytes(&self, name: &str) -> Result<Vec<u8>> {
        if !self.backup_names()?.iter().any(|n| n == name) {
            bail!("there is no backup named {name:?}");
        }
        fs::read(self.backup_dir().join(name)).context("reading the backup")
    }

    /// Replaces the collection's cards with a backup file's, once it has been read and
    /// checked and `before` has backed up the current collection. Returns `before`'s
    /// answer.
    pub fn restore_from(&self, bytes: &[u8], before: impl FnOnce() -> Result<String>) -> Result<String> {
        let restored: StoreData = serde_json::from_slice(bytes).context("the backup is damaged")?;
        let game = self.file.read(|d| d.game.clone());
        if !restored.game.is_empty() && restored.game != game {
            bail!("that backup is a {:?} collection, not {game:?}", restored.game);
        }
        let before = before()?;
        self.file.mutate(|d| {
            d.cards = restored.cards;
            Ok(())
        })?;
        tracing::info!("collection restored");
        Ok(before)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::BulkChange;
    use crate::collection::store::tests::card;

    #[test]
    fn backups_keep_ten_and_restore() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(dir.path().join("mtg/collection.json"), "mtg").unwrap();
        s.add_cards(vec![card("a", 1)]).unwrap();
        let first = s.backup("import").unwrap();
        s.add_cards(vec![card("b", 1)]).unwrap();
        for _ in 0..12 {
            s.backup("bulk-edit").unwrap(); // more than are kept, in one second, never overwriting
        }
        let list = s.backups().unwrap();
        assert_eq!(list.len(), BACKUPS_KEPT);
        assert!(list.iter().all(|b| b.name != first), "the oldest went");
        assert_eq!((list[0].reason.as_str(), list[0].cards), ("bulk-edit", 2));
        assert!(list[0].size > 0 && list[0].date.len() == 19);

        let kept = s.backups().unwrap().last().unwrap().name.clone();
        s.bulk_edit(&["a".into(), "b".into()], &BulkChange { delete: true, ..Default::default() }).unwrap();
        let taken = s.restore(&kept).unwrap();
        assert_eq!(s.ids(), ["a", "b"]);
        let after = s.backups().unwrap();
        assert_eq!((after[0].name.as_str(), after[0].reason.as_str(), after[0].cards), (taken.as_str(), "restore", 0));

        for bad in ["../collection.json", "collection.json", "nope", &format!("../backups/{kept}")] {
            assert!(s.restore(bad).is_err(), "{bad}");
        }
        assert!(s.backup("whatever").is_err());
    }

    #[test]
    fn backup_names() {
        assert_eq!(parse_backup_name("collection-20261114-093005-import.json"), Some(("2026-11-14 09:30:05".into(), 1, "import".into())));
        assert_eq!(
            parse_backup_name("collection-20261114-093005-12-bulk-delete.json").unwrap(),
            ("2026-11-14 09:30:05".into(), 12, "bulk-delete".into())
        );
        assert_eq!(parse_backup_name("collection-20261114-093005-bulk-delete.json").unwrap().1, 1);
        assert_eq!(parse_backup_name("collection-20261114-093005-other.json"), None);
        assert_eq!(parse_backup_name("notes.txt"), None);
    }
}
