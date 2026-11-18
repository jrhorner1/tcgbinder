//! JSON files that this app and other devices (through a synced folder) both change.
//!
//! Every change is saved before the method making it returns, so the data in memory
//! never holds anything the file doesn't. That makes another device's changes easy to
//! take in without merging: before applying a change, if the file on disk has changed,
//! reload it, then apply the change to what was loaded and save.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::Hasher;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, RwLock};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result, anyhow};
use serde::Serialize;
use serde::de::DeserializeOwned;

/// The error when a file changed on disk but can't be read: it is most likely still
/// arriving from another device, so nothing is saved over it.
#[derive(Debug)]
pub struct FileBusy {
    pub path: PathBuf,
}

impl std::fmt::Display for FileBusy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = self.path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
        write!(f, "{name} is being updated by another device; try again in a moment")
    }
}

impl std::error::Error for FileBusy {}

/// What a file looked like when last read or written. The default is "no file".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Stamp {
    modified: Option<SystemTime>,
    len: u64,
    hash: u64,
}

/// A file modified this recently is read and hashed even when its time and length
/// match the stamp, since some file systems only keep times to the second or two.
const COARSE_CLOCK: Duration = Duration::from_secs(2);

impl Stamp {
    /// The stamp of `bytes`, just read from or written to `path`.
    pub(crate) fn of(path: &Path, bytes: &[u8]) -> Stamp {
        let meta = fs::metadata(path).ok();
        Stamp { modified: meta.as_ref().and_then(|m| m.modified().ok()), len: bytes.len() as u64, hash: hash(bytes) }
    }

    /// Whether this stamp is of an existing file.
    fn exists(&self) -> bool {
        *self != Stamp::default()
    }

    /// Reads `path` if it changed since this stamp. `Ok(None)`: unchanged (a file
    /// touched without changing its contents counts as unchanged). `Ok(Some)`: its new
    /// contents and their stamp. A file that has disappeared is a [`FileBusy`] error.
    pub(crate) fn read_if_changed(&mut self, path: &Path) -> Result<Option<(Vec<u8>, Stamp)>> {
        let meta = match fs::metadata(path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return if self.exists() { Err(FileBusy { path: path.into() }.into()) } else { Ok(None) };
            }
            Err(e) => return Err(e).with_context(|| format!("checking {}", path.display())),
        };
        let modified = meta.modified().ok();
        let recent = modified.and_then(|m| m.elapsed().ok()).is_none_or(|age| age < COARSE_CLOCK);
        if self.exists() && modified == self.modified && meta.len() == self.len && !recent {
            return Ok(None);
        }
        let bytes = match fs::read(path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(FileBusy { path: path.into() }.into()),
            Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
        };
        let stamp = Stamp { modified, len: bytes.len() as u64, hash: hash(&bytes) };
        if self.exists() && stamp.hash == self.hash {
            *self = stamp; // touched, not changed
            return Ok(None);
        }
        Ok(Some((bytes, stamp)))
    }
}

/// A hash of a file's contents.
fn hash(bytes: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    h.write(bytes);
    h.finish()
}

/// Writes `bytes` to `path` atomically: a temporary file named `<prefix>*.tmp` in the
/// same folder is written, synced and renamed over the target, so a crash mid-save
/// never leaves a damaged file. Returns the new file's stamp.
pub(crate) fn write_atomic(path: &Path, prefix: &str, bytes: &[u8]) -> Result<Stamp> {
    let dir = path.parent().ok_or_else(|| anyhow!("no folder for {}", path.display()))?;
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let mut tmp = tempfile::Builder::new().prefix(prefix).suffix(".tmp").tempfile_in(dir)?;
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all().with_context(|| format!("syncing {}", path.display()))?;
    tmp.persist(path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(Stamp::of(path, bytes))
}

/// A JSON document on disk that this process and others (through a synced folder)
/// both change. Every mutation is saved before it returns.
pub struct JsonFile<T> {
    path: PathBuf,
    pretty: bool,
    data: RwLock<T>,
    /// Held for the whole of a refresh or mutation, so saves land in order.
    stamp: Mutex<Stamp>,
    /// Counts changes, from this process or reloaded from disk.
    revision: AtomicU64,
}

impl<T: Serialize + DeserializeOwned + Default + Clone> JsonFile<T> {
    /// Reads the file at `path`. A missing or blank file gives the default value; a file
    /// that doesn't parse is an error. `pretty` indents what is saved.
    pub fn open(path: impl AsRef<Path>, pretty: bool) -> Result<JsonFile<T>> {
        let path = std::path::absolute(path.as_ref()).with_context(|| format!("resolving {}", path.as_ref().display()))?;
        let (data, stamp) = match fs::read(&path) {
            Ok(b) if b.iter().all(u8::is_ascii_whitespace) => (T::default(), Stamp::of(&path, &b)),
            Ok(b) => (serde_json::from_slice(&b).with_context(|| format!("malformed {}", path.display()))?, Stamp::of(&path, &b)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (T::default(), Stamp::default()),
            Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
        };
        Ok(JsonFile { path, pretty, data: RwLock::new(data), stamp: Mutex::new(stamp), revision: AtomicU64::new(0) })
    }

    /// The absolute path of the file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Runs `f` on the data as last read or saved.
    pub fn read<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&self.data.read().unwrap())
    }

    /// Reloads if the file changed, applies `f`, and saves atomically. If `f` fails,
    /// nothing is saved and the data is unchanged.
    pub fn mutate<R>(&self, f: impl FnOnce(&mut T) -> Result<R>) -> Result<R> {
        let mut stamp = self.stamp.lock().unwrap();
        self.reload_if_changed(&mut stamp)?;
        let mut next = self.data.read().unwrap().clone();
        let out = f(&mut next)?;
        let mut bytes = if self.pretty { serde_json::to_vec_pretty(&next)? } else { serde_json::to_vec(&next)? };
        bytes.push(b'\n');
        *stamp = write_atomic(&self.path, &self.temp_prefix(), &bytes)?;
        *self.data.write().unwrap() = next;
        self.revision.fetch_add(1, Ordering::Relaxed);
        Ok(out)
    }

    /// Reloads if the file changed on disk. True if it did.
    pub fn refresh(&self) -> Result<bool> {
        self.reload_if_changed(&mut self.stamp.lock().unwrap())
    }

    /// A number that changes whenever the data does.
    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::Relaxed)
    }

    fn reload_if_changed(&self, stamp: &mut Stamp) -> Result<bool> {
        let Some((bytes, next)) = stamp.read_if_changed(&self.path)? else { return Ok(false) };
        // A file that doesn't parse (or is empty) is most likely half-synced: keep what
        // we have and don't save over it.
        let data: T = serde_json::from_slice(&bytes).map_err(|_| FileBusy { path: self.path.clone() })?;
        *self.data.write().unwrap() = data;
        *stamp = next;
        self.revision.fetch_add(1, Ordering::Relaxed);
        tracing::info!(path = %self.path.display(), "reloaded a file changed on disk");
        Ok(true)
    }

    /// Temporary files are named after the file: `collection_*.tmp`.
    fn temp_prefix(&self) -> String {
        let stem = self.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        format!("{stem}_")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    type Doc = BTreeMap<String, i64>;

    fn age(path: &Path) {
        // Back-date the file so the coarse-clock rule doesn't force a read.
        let old = SystemTime::now() - Duration::from_secs(60);
        fs::File::options().write(true).open(path).unwrap().set_modified(old).unwrap();
    }

    #[test]
    fn two_handles_see_each_other() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.json");
        let a: JsonFile<Doc> = JsonFile::open(&path, true).unwrap();
        let b: JsonFile<Doc> = JsonFile::open(&path, false).unwrap();
        a.mutate(|d| Ok(d.insert("a".into(), 1))).unwrap();
        b.mutate(|d| Ok(d.insert("b".into(), 2))).unwrap();
        a.mutate(|d| Ok(d.insert("c".into(), 3))).unwrap();
        assert!(b.refresh().unwrap());
        assert_eq!(b.read(|d| d.len()), 3);
        assert_eq!(a.read(|d| d.clone()), b.read(|d| d.clone()));
    }

    #[test]
    fn failed_change_leaves_data() {
        let dir = tempfile::tempdir().unwrap();
        let f: JsonFile<Doc> = JsonFile::open(dir.path().join("doc.json"), true).unwrap();
        f.mutate(|d| Ok(d.insert("a".into(), 1))).unwrap();
        let rev = f.revision();
        let err = f.mutate(|d| -> Result<()> {
            d.insert("b".into(), 2);
            anyhow::bail!("no")
        });
        assert!(err.is_err());
        assert_eq!((f.read(|d| d.len()), f.revision()), (1, rev));
        let names: Vec<_> = fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names, ["doc.json"]);
    }

    #[test]
    fn half_written_or_missing_file_is_busy() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.json");
        let f: JsonFile<Doc> = JsonFile::open(&path, true).unwrap();
        f.mutate(|d| Ok(d.insert("alpha".into(), 1))).unwrap();
        let whole = fs::read(&path).unwrap();
        fs::write(&path, &whole[..whole.len() / 2]).unwrap();
        let err = f.mutate(|d| Ok(d.insert("beta".into(), 2))).unwrap_err();
        assert!(err.downcast_ref::<FileBusy>().is_some(), "{err}");
        assert!(err.to_string().contains("being updated by another device"));
        assert_eq!(fs::read(&path).unwrap(), &whole[..whole.len() / 2]);
        assert_eq!(f.read(|d| d.len()), 1);

        fs::remove_file(&path).unwrap();
        assert!(f.refresh().unwrap_err().downcast_ref::<FileBusy>().is_some());
        assert!(!path.exists());
    }

    #[test]
    fn touched_file_is_not_reloaded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.json");
        let f: JsonFile<Doc> = JsonFile::open(&path, true).unwrap();
        f.mutate(|d| Ok(d.insert("a".into(), 1))).unwrap();
        age(&path);
        let rev = f.revision();
        assert!(!f.refresh().unwrap());
        fs::File::options().write(true).open(&path).unwrap().set_modified(SystemTime::now()).unwrap();
        assert!(!f.refresh().unwrap());
        assert_eq!(f.revision(), rev);
    }
}
