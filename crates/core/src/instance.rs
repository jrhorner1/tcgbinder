//! Makes sure only one copy of the app uses a data folder at a time, with an OS file
//! lock that is released automatically when the app exits or crashes. The lock file
//! is in the local folder (`datadir::lock_path`).

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

/// Why the lock couldn't be taken.
#[derive(Debug)]
pub enum LockError {
    /// Another copy holds it (with its process ID, when known).
    Running(Option<u32>),
    Io(std::io::Error),
}

impl std::fmt::Display for LockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LockError::Running(Some(pid)) => write!(f, "another copy of the app is already running (process {pid})"),
            LockError::Running(None) => f.write_str("another copy of the app is already running"),
            LockError::Io(e) => write!(f, "cannot lock the data folder: {e}"),
        }
    }
}

impl std::error::Error for LockError {}

/// A held single-instance lock. Keep it for the life of the process; dropping it
/// releases the lock (the file stays, to be reused next run).
pub struct Lock {
    _file: File,
}

/// Takes the lock file at `path` (which holds the owner's process ID) without waiting.
pub fn acquire(path: &Path) -> Result<Lock, LockError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(LockError::Io)?;
    }
    // No truncate: the PID inside belongs to the current owner until we hold the lock.
    let mut f = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path).map_err(LockError::Io)?;
    match f.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => {
            let mut s = String::new();
            let _ = f.read_to_string(&mut s);
            return Err(LockError::Running(s.trim().parse().ok()));
        }
        Err(std::fs::TryLockError::Error(e)) => return Err(LockError::Io(e)),
    }
    if f.set_len(0).is_ok() && f.seek(SeekFrom::Start(0)).is_ok() {
        let _ = writeln!(f, "{}", std::process::id());
        let _ = f.sync_all();
    }
    Ok(Lock { _file: f })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_lock_fails_until_released() {
        let dir = tempfile::tempdir().unwrap();
        let path = crate::datadir::lock_path(dir.path(), Path::new("/some/data"));
        assert!(path.starts_with(dir.path().join("locks")));
        assert_ne!(path, crate::datadir::lock_path(dir.path(), Path::new("/other/data")));
        let lock = acquire(&path).unwrap();
        match acquire(&path) {
            // Windows doesn't let another handle read a locked file, so the PID may be unknown.
            Err(LockError::Running(pid)) => assert!(pid == Some(std::process::id()) || (cfg!(windows) && pid.is_none())),
            Err(e) => panic!("unexpected error: {e}"),
            Ok(_) => panic!("locked twice"),
        }
        drop(lock);
        assert!(acquire(&path).is_ok());
    }
}
