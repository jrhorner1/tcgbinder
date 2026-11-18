//! The user's app settings (appearance, price updates, defaults for new cards, ...),
//! shared by the desktop, phone and web apps. They are stored as settings.json in the
//! data folder, so they move with the collection.

use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::jsonfile::JsonFile;

/// The settings file in the data folder.
pub const FILE_NAME: &str = "settings.json";

/// The choices for each setting with a fixed set of values.
pub mod choices {
    /// The Value Over Time ranges the front ends offer.
    pub const CHART_RANGES: &[&str] = &["30D", "90D", "1Y", "All"];
    pub const CONDITIONS: &[&str] = &["NM", "LP", "MP", "HP", "DMG"];

    /// Every setting with fixed choices, by its JSON name.
    pub const ALL: &[(&str, &[&str])] = &[
        ("theme", &["system", "light", "dark"]),
        ("text_size", &["small", "normal", "large", "larger"]),
        ("card_order", &["name", "newest", "oldest", "value"]),
        ("set_completion", &["main", "all"]),
        ("chart_range", CHART_RANGES),
        ("price_refresh", &["daily", "weekly", "manual"]),
        ("default_condition", CONDITIONS),
        ("default_date", &["today", "none"]),
        ("log_level", &["debug", "info", "warn", "error"]),
    ];
}

/// The user's choices. Fields missing from the file keep their defaults.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    // Appearance
    /// system, light or dark.
    pub theme: String,
    /// small, normal, large or larger.
    pub text_size: String,
    /// Download and show card images.
    pub show_images: bool,

    // Collection
    /// The order of the collection's cards: name, by set (newest/oldest), value.
    pub card_order: String,
    /// The value-over-time range shown first.
    pub chart_range: String,
    /// Set completion counts the printed set (main: collector numbers up to the
    /// number printed on the cards) or every card Scryfall lists in it (all).
    pub set_completion: String,

    // Prices
    /// daily, weekly or manual.
    pub price_refresh: String,
    /// Load price history automatically the first time (desktop and web).
    pub auto_price_history: bool,

    // New cards
    /// The condition preselected when adding a card.
    pub default_condition: String,
    /// The acquired date when adding a card: today or none.
    pub default_date: String,

    // Advanced
    /// Ask before deleting cards.
    pub confirm_delete: bool,
    /// debug, info, warn or error.
    pub log_level: String,

    // Data
    /// The data folder this device uses; "" for the local folder. Changed only through
    /// the data folder API, which moves or checks the data first.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub data_dir: String,
}

impl Default for Settings {
    /// The settings before the user changes anything.
    fn default() -> Self {
        Settings {
            theme: "system".into(),
            text_size: "normal".into(),
            show_images: true,
            card_order: "name".into(),
            chart_range: "90D".into(),
            set_completion: "main".into(),
            price_refresh: "daily".into(),
            auto_price_history: true,
            default_condition: "NM".into(),
            default_date: "today".into(),
            confirm_delete: true,
            log_level: "info".into(),
            data_dir: String::new(),
        }
    }
}

impl Settings {
    fn fields_mut(&mut self) -> [(&'static str, &mut String); 9] {
        [
            ("theme", &mut self.theme),
            ("text_size", &mut self.text_size),
            ("card_order", &mut self.card_order),
            ("chart_range", &mut self.chart_range),
            ("set_completion", &mut self.set_completion),
            ("price_refresh", &mut self.price_refresh),
            ("default_condition", &mut self.default_condition),
            ("default_date", &mut self.default_date),
            ("log_level", &mut self.log_level),
        ]
    }

    fn choices_for(name: &str) -> &'static [&'static str] {
        choices::ALL.iter().find(|(n, _)| *n == name).map_or(&[], |(_, c)| c)
    }

    /// Replaces values that aren't valid choices with the defaults.
    fn normalize(&mut self) {
        let mut defaults = Settings::default();
        for ((name, value), (_, default)) in self.fields_mut().into_iter().zip(defaults.fields_mut()) {
            if !Self::choices_for(name).contains(&value.as_str()) {
                std::mem::swap(value, default);
            }
        }
    }

    /// Checks that every value is one of its setting's choices.
    pub fn validate(&self) -> Result<()> {
        let mut copy = self.clone();
        for (name, value) in copy.fields_mut() {
            let allowed = Self::choices_for(name);
            if !allowed.contains(&value.as_str()) {
                bail!("{name} must be one of {}, not {value:?}", allowed.join(", "));
            }
        }
        Ok(())
    }

    /// These settings with the fields in `change` (a JSON object) replaced. Fields left
    /// out keep their values; unknown fields are ignored.
    pub fn with_changes(&self, change: &Value) -> Result<Settings> {
        let Value::Object(change) = change else { bail!("settings must be a JSON object") };
        if change.contains_key("data_dir") {
            bail!("the data folder is changed in Settings → Data, which moves or checks the collection first");
        }
        let mut v = serde_json::to_value(self)?;
        if let Value::Object(map) = &mut v {
            for (k, val) in change {
                if map.contains_key(k) {
                    map.insert(k.clone(), val.clone());
                }
            }
        }
        Ok(serde_json::from_value(v)?)
    }
}

type Listener = Arc<dyn Fn(&Settings, &Settings) + Send + Sync>;

/// Where the settings live: a file, or memory only (tests).
enum Backing {
    File(JsonFile<Settings>),
    Memory(Mutex<Settings>),
}

/// Keeps the settings and saves every change.
pub struct SettingsStore {
    backing: Backing,
    listeners: Mutex<Vec<Listener>>,
}

impl SettingsStore {
    /// Reads the settings in `dir` (the defaults if there is no file yet). A file that
    /// can't be read is set aside and the defaults are used, so the app always starts.
    pub fn open(dir: &Path) -> SettingsStore {
        let path = dir.join(FILE_NAME);
        let file = JsonFile::open(&path, true).or_else(|e| {
            let bad = path.with_extension("json.unreadable");
            tracing::warn!(path = %path.display(), moved_to = %bad.display(), error = %e, "settings file is damaged; using defaults");
            let _ = std::fs::rename(&path, &bad);
            JsonFile::open(&path, true)
        });
        let backing = match file {
            Ok(f) => Backing::File(f),
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "cannot read settings; using defaults");
                Backing::Memory(Mutex::default())
            }
        };
        SettingsStore { backing, listeners: Mutex::default() }
    }

    /// A store that isn't saved anywhere (tests).
    pub fn memory() -> SettingsStore {
        SettingsStore { backing: Backing::Memory(Mutex::default()), listeners: Mutex::default() }
    }

    /// The current settings. Values that aren't valid choices read as the defaults.
    pub fn get(&self) -> Settings {
        let mut s = match &self.backing {
            Backing::File(f) => f.read(Settings::clone),
            Backing::Memory(m) => m.lock().unwrap().clone(),
        };
        s.normalize();
        s
    }

    /// The settings file (`None` for a memory store).
    pub fn path(&self) -> Option<&Path> {
        match &self.backing {
            Backing::File(f) => Some(f.path()),
            Backing::Memory(_) => None,
        }
    }

    /// Validates and saves new settings, then tells the listeners.
    pub fn set(&self, s: Settings) -> Result<()> {
        s.validate()?;
        let old = self.get();
        if old == s {
            return Ok(());
        }
        match &self.backing {
            Backing::File(f) => f.mutate(|cur| {
                *cur = s.clone();
                Ok(())
            })?,
            Backing::Memory(m) => *m.lock().unwrap() = s.clone(),
        }
        tracing::info!(settings = ?s, "settings changed");
        let listeners = self.listeners.lock().unwrap().clone();
        for f in listeners {
            f(&old, &s);
        }
        Ok(())
    }

    /// Calls `f` after every change, with the settings before and after.
    pub fn on_change(&self, f: impl Fn(&Settings, &Settings) + Send + Sync + 'static) {
        self.listeners.lock().unwrap().push(Arc::new(f));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[test]
    fn open_set_and_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let st = SettingsStore::open(dir.path());
        assert_eq!(st.get(), Settings::default());
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = calls.clone();
        st.on_change(move |old, new| {
            assert_eq!(old.theme, "system");
            assert_eq!(new.theme, "dark");
            seen.fetch_add(1, Ordering::SeqCst);
        });
        let themed = |theme: &str| Settings { theme: theme.into(), ..Settings::default() };
        st.set(themed("dark")).unwrap();
        st.set(themed("dark")).unwrap(); // unchanged: no call
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(st.set(themed("purple")).is_err());
        assert_eq!(SettingsStore::open(dir.path()).get().theme, "dark");
    }

    #[test]
    fn bad_values_and_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(FILE_NAME), r#"{"theme":"neon","show_images":false}"#).unwrap();
        let s = SettingsStore::open(dir.path()).get();
        assert_eq!((s.theme.as_str(), s.show_images), ("system", false));

        std::fs::write(dir.path().join(FILE_NAME), "not json").unwrap();
        assert_eq!(SettingsStore::open(dir.path()).get(), Settings::default());
        assert!(dir.path().join("settings.json.unreadable").exists());
    }

    #[test]
    fn partial_changes() {
        let s = Settings::default();
        let next = s.with_changes(&serde_json::json!({"card_order": "value", "nope": 1})).unwrap();
        assert_eq!(next.card_order, "value");
        assert_eq!(next.theme, "system");
        assert!(s.with_changes(&serde_json::json!({"show_images": "yes"})).is_err());
        assert!(s.with_changes(&serde_json::json!([1])).is_err());
        assert!(s.with_changes(&serde_json::json!({"data_dir": "/tmp"})).is_err());
    }
}
