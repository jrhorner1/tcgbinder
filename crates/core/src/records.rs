//! Records: the small JSON files kept beside the collection, one per kind of record
//! (lots, lists, the watchlist, sealed product, sales). Each file holds one list,
//! `{"records": [...]}`, read and saved through [`JsonFile`] so other devices'
//! changes are taken in. A feature's own operations are `impl Records<Thing>` blocks
//! in that feature's module, built on [`Records::update`] and [`Records::update_all`].
//!
//! Also here: [`DataFile`], what every file the service keeps has in common, so the
//! service can refresh them and count their changes in one loop.

use std::cmp::Ordering;
use std::path::Path;

use anyhow::{Result, bail};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::jsonfile::JsonFile;

/// One kind of record kept in its own JSON file beside the collection: a lot, a
/// list, a watch, a sealed item or a sale.
pub trait Record: Clone + PartialEq + Serialize + DeserializeOwned + Send + Sync {
    /// The file's name, e.g. "lots.json".
    const FILE: &'static str;
    /// What one record is called in messages: "there is no lot ...".
    const NAME: &'static str;
    /// What identifies a record. Usually its `id` field.
    fn id(&self) -> String;
    /// Gives a new record its ID (and anything else a new record starts with, such as
    /// its creation time). The default does nothing, for records whose ID is made from
    /// their own fields.
    fn assign_id(&mut self) {}
    /// Checks (and tidies) a record before it is added or saved.
    fn check(&mut self) -> Result<()> {
        Ok(())
    }
    /// The order [`Records::all`] lists records in. The default keeps the file's order.
    fn order(a: &Self, b: &Self) -> Ordering {
        let _ = (a, b);
        Ordering::Equal
    }
    /// Combines two versions of one record when merging a conflict copy. `theirs`
    /// comes from the other device. The default keeps ours.
    fn merge(&mut self, theirs: Self) {
        let _ = theirs;
    }
}

/// A records file's contents.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(bound(serialize = "T: Serialize", deserialize = "T: DeserializeOwned"))]
pub struct RecordsFile<T> {
    #[serde(default = "Vec::new")]
    pub records: Vec<T>,
}

impl<T> Default for RecordsFile<T> {
    fn default() -> Self {
        RecordsFile { records: Vec::new() }
    }
}

/// Every record of one kind, saved on each change.
pub struct Records<T: Record> {
    file: JsonFile<RecordsFile<T>>,
}

impl<T: Record> Records<T> {
    /// Opens (or starts) the records file in a game's data folder.
    pub fn open(game_dir: &Path) -> Result<Self> {
        Ok(Records { file: JsonFile::open(game_dir.join(T::FILE), true)? })
    }

    /// Every record, in [`Record::order`].
    pub fn all(&self) -> Vec<T> {
        let mut all = self.file.read(|d| d.records.clone());
        all.sort_by(T::order);
        all
    }

    /// The record with this ID.
    pub fn get(&self, id: &str) -> Option<T> {
        self.file.read(|d| d.records.iter().find(|r| r.id() == id).cloned())
    }

    /// Adds a new record (assigning its ID) and returns it as saved.
    pub fn add(&self, mut record: T) -> Result<T> {
        record.check()?;
        record.assign_id();
        self.file.mutate(|d| {
            if d.records.iter().any(|r| r.id() == record.id()) {
                bail!("there is already a {} {:?}", T::NAME, record.id());
            }
            d.records.push(record.clone());
            Ok(())
        })?;
        Ok(record)
    }

    /// Replaces the record with the same ID; an error if there is none.
    pub fn save(&self, mut record: T) -> Result<T> {
        record.check()?;
        let id = record.id();
        self.update(&id, |r| {
            *r = record.clone();
            Ok(())
        })?;
        Ok(record)
    }

    /// Changes one record in place; an error if there is none.
    pub fn update<R>(&self, id: &str, f: impl FnOnce(&mut T) -> Result<R>) -> Result<R> {
        self.file.mutate(|d| match d.records.iter_mut().find(|r| r.id() == id) {
            Some(r) => f(r),
            None => bail!("there is no {} {id:?}", T::NAME),
        })
    }

    /// Changes any number of records in one save.
    pub fn update_all<R>(&self, f: impl FnOnce(&mut Vec<T>) -> Result<R>) -> Result<R> {
        self.file.mutate(|d| f(&mut d.records))
    }

    /// Runs `f` on every record, without copying them.
    pub fn read<R>(&self, f: impl FnOnce(&[T]) -> R) -> R {
        self.file.read(|d| f(&d.records))
    }

    /// Deletes a record, returning it; an error if there is none.
    pub fn remove(&self, id: &str) -> Result<T> {
        self.file.mutate(|d| match d.records.iter().position(|r| r.id() == id) {
            Some(i) => Ok(d.records.remove(i)),
            None => bail!("there is no {} {id:?}", T::NAME),
        })
    }

    /// Adds records from a conflict copy: new IDs are added, shared IDs are merged
    /// with [`Record::merge`]. Returns how many were added or changed.
    pub fn merge_copy(&self, theirs: Vec<T>) -> Result<usize> {
        self.file.mutate(|d| {
            let mut changed = 0;
            for t in theirs {
                match d.records.iter_mut().find(|r| r.id() == t.id()) {
                    Some(ours) => {
                        let before = ours.clone();
                        ours.merge(t);
                        if *ours != before {
                            changed += 1;
                        }
                    }
                    None => {
                        d.records.push(t);
                        changed += 1;
                    }
                }
            }
            Ok(changed)
        })
    }

    /// The file's path.
    pub fn path(&self) -> &Path {
        self.file.path()
    }
}

/// A data file the service keeps: the collection, the price history, or a records
/// file.
pub trait DataFile: Send + Sync {
    /// The file's name, e.g. "lots.json".
    fn file_name(&self) -> &str;
    /// Its path.
    fn path(&self) -> &Path;
    /// Takes in changes another device made. True if there were any.
    fn refresh(&self) -> Result<bool>;
    /// A number that changes whenever the data does.
    fn revision(&self) -> u64;
}

impl<T: Record> DataFile for Records<T> {
    fn file_name(&self) -> &str {
        T::FILE
    }
    fn path(&self) -> &Path {
        self.file.path()
    }
    fn refresh(&self) -> Result<bool> {
        self.file.refresh()
    }
    fn revision(&self) -> u64 {
        self.file.revision()
    }
}

impl DataFile for crate::collection::Store {
    fn file_name(&self) -> &str {
        crate::datadir::COLLECTION_FILE
    }
    fn path(&self) -> &Path {
        self.file_path()
    }
    fn refresh(&self) -> Result<bool> {
        crate::collection::Store::refresh(self)
    }
    fn revision(&self) -> u64 {
        crate::collection::Store::revision(self)
    }
}

impl DataFile for crate::collection::PriceHistory {
    fn file_name(&self) -> &str {
        crate::datadir::PRICE_HISTORY_FILE
    }
    fn path(&self) -> &Path {
        crate::collection::PriceHistory::path(self)
    }
    fn refresh(&self) -> Result<bool> {
        let before = crate::collection::PriceHistory::revision(self);
        crate::collection::PriceHistory::refresh(self)?;
        Ok(crate::collection::PriceHistory::revision(self) != before)
    }
    fn revision(&self) -> u64 {
        crate::collection::PriceHistory::revision(self)
    }
}

/// A random ID: 16 hexadecimal characters.
pub fn new_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    // RandomState is seeded randomly per process; mixing in the time and a counter
    // makes each ID differ.
    static COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
    h.write_u64(COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    format!("{:016x}", h.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    struct Note {
        id: String,
        text: String,
        count: i64,
    }

    impl Record for Note {
        const FILE: &'static str = "notes.json";
        const NAME: &'static str = "note";
        fn id(&self) -> String {
            self.id.clone()
        }
        fn assign_id(&mut self) {
            self.id = new_id();
        }
        fn check(&mut self) -> Result<()> {
            self.text = self.text.trim().to_string();
            if self.text.is_empty() {
                bail!("empty");
            }
            Ok(())
        }
        fn order(a: &Self, b: &Self) -> Ordering {
            a.text.cmp(&b.text)
        }
        fn merge(&mut self, theirs: Self) {
            self.count = self.count.max(theirs.count);
        }
    }

    fn note(text: &str) -> Note {
        Note { text: text.into(), ..Note::default() }
    }

    fn age(path: &Path) {
        // Back-date the file so the coarse-clock rule doesn't force a read.
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(60);
        std::fs::File::options().write(true).open(path).unwrap().set_modified(old).unwrap();
    }

    #[test]
    fn add_save_update_remove() {
        let dir = tempfile::tempdir().unwrap();
        let r: Records<Note> = Records::open(dir.path()).unwrap();
        let b = r.add(note(" b ")).unwrap();
        assert_eq!((b.id.len(), b.text.as_str()), (16, "b"), "an ID is assigned and the record checked");
        assert!(r.add(note(" ")).is_err());
        let a = r.add(note("a")).unwrap();
        assert_eq!(r.all().iter().map(|n| n.text.as_str()).collect::<Vec<_>>(), ["a", "b"], "in order");

        r.save(Note { text: "c".into(), ..b.clone() }).unwrap();
        assert_eq!(r.get(&b.id).unwrap().text, "c");
        assert!(r.save(Note { id: "nope".into(), ..note("x") }).unwrap_err().to_string().contains("there is no note"));
        assert_eq!(r.update(&a.id, |n| Ok(std::mem::replace(&mut n.count, 5))).unwrap(), 0);
        assert_eq!(r.get(&a.id).unwrap().count, 5);
        assert!(r.update("nope", |_| Ok(())).is_err());
        assert_eq!(r.update_all(|all| Ok(all.len())).unwrap(), 2);
        assert_eq!(r.remove(&a.id).unwrap().text, "a");
        assert!(r.remove(&a.id).is_err());
        let text = std::fs::read_to_string(r.path()).unwrap();
        assert!(text.contains("\"records\""), "{text}");
    }

    #[test]
    fn merge_adds_and_merges() {
        let dir = tempfile::tempdir().unwrap();
        let r: Records<Note> = Records::open(dir.path()).unwrap();
        let a = r.add(Note { count: 2, ..note("a") }).unwrap();
        let theirs = vec![Note { count: 1, ..a.clone() }, Note { id: "t".into(), ..note("t") }];
        assert_eq!(r.merge_copy(theirs).unwrap(), 1, "only the new one: ours has the larger count");
        assert_eq!(r.merge_copy(vec![Note { count: 9, ..a.clone() }]).unwrap(), 1);
        assert_eq!((r.get(&a.id).unwrap().count, r.all().len()), (9, 2));
    }

    #[test]
    fn a_second_handle_sees_changes() {
        let dir = tempfile::tempdir().unwrap();
        let here: Records<Note> = Records::open(dir.path()).unwrap();
        let there: Records<Note> = Records::open(dir.path()).unwrap();
        let a = here.add(note("a")).unwrap();
        age(here.path());
        let rev = there.revision();
        assert!(there.refresh().unwrap());
        assert!(there.revision() > rev);
        assert_eq!(there.get(&a.id).unwrap().text, "a");
        there.add(note("b")).unwrap();
        assert!(here.refresh().unwrap());
        assert_eq!(here.all().len(), 2);
        assert_eq!(here.file_name(), "notes.json");
    }
}
