use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::Client;
use tcgbinder_core::collection::SetInfo;
use tcgbinder_core::dates;

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct SetsCache {
    /// `YYYY-MM-DD`.
    fetched: String,
    sets: Vec<SetInfo>,
}

/// How long the cached set list is used before it is downloaded again.
const MAX_AGE_DAYS: i64 = 7;

fn index(list: Vec<SetInfo>) -> HashMap<String, SetInfo> {
    list.into_iter().map(|s| (s.code.to_uppercase(), s)).collect()
}

impl Client {
    /// Every set, keyed by uppercase set code.
    pub fn fetch_sets(&self) -> Result<HashMap<String, SetInfo>> {
        #[derive(Deserialize)]
        struct Resp {
            data: Vec<SetInfo>,
        }
        Ok(index(self.get::<Resp>("/sets")?.data))
    }
}

/// Set metadata from the cache file at `path`, refreshed from Scryfall when it is
/// missing or older than a week. If Scryfall can't be reached, a stale cache is still
/// used.
pub fn load_sets(c: &Client, path: &Path) -> Result<HashMap<String, SetInfo>> {
    let cache: SetsCache = std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    let fresh_enough = dates::parse(&cache.fetched).is_some_and(|d| (dates::today() - d).num_days() < MAX_AGE_DAYS);
    if !cache.sets.is_empty() && fresh_enough {
        return Ok(index(cache.sets));
    }
    match c.fetch_sets() {
        Ok(fresh) => {
            let mut sets: Vec<SetInfo> = fresh.values().cloned().collect();
            sets.sort_by(|a, b| a.code.cmp(&b.code));
            if let Ok(b) = serde_json::to_vec(&SetsCache { fetched: dates::today_string(), sets }) {
                let _ = std::fs::write(path, b);
            }
            Ok(fresh)
        }
        Err(_) if !cache.sets.is_empty() => Ok(index(cache.sets)),
        Err(e) => Err(e),
    }
}
