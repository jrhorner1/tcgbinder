//! The app's logging (`tracing`): human-readable lines to a log file and, optionally,
//! stderr, with a minimum level that can change while the app runs.

use std::fs::OpenOptions;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use anyhow::{Result, bail};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{Registry, fmt, reload};

/// The size at which the log file is rotated to `<file>.1` at startup.
pub const MAX_SIZE: u64 = 10 << 20;

/// How to set up logging.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// The log file; `None` for no file.
    pub file: Option<PathBuf>,
    /// debug, info, warn or error.
    pub level: String,
    /// Also write to stderr.
    pub stderr: bool,
}

/// Converts a level name to a filter.
pub fn parse_level(s: &str) -> Result<LevelFilter> {
    Ok(match s.trim().to_lowercase().as_str() {
        "debug" => LevelFilter::DEBUG,
        "" | "info" => LevelFilter::INFO,
        "warn" | "warning" => LevelFilter::WARN,
        "error" => LevelFilter::ERROR,
        _ => bail!("unknown log level {s:?} (use debug, info, warn or error)"),
    })
}

static LEVEL: OnceLock<reload::Handle<LevelFilter, Registry>> = OnceLock::new();

/// Changes the minimum level of the logger [`setup`] installed.
pub fn set_level(name: &str) -> Result<()> {
    let level = parse_level(name)?;
    if let Some(handle) = LEVEL.get() {
        handle.modify(|f| *f = level)?;
    }
    Ok(())
}

/// Installs the global logger. Call it once, at startup.
pub fn setup(o: &Options) -> Result<()> {
    let (filter, handle) = reload::Layer::new(parse_level(&o.level)?);
    let file = match &o.file {
        Some(path) => {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            // Keep one previous log when the file has grown past MAX_SIZE.
            if std::fs::metadata(path).is_ok_and(|m| m.len() > MAX_SIZE) {
                let _ = std::fs::rename(path, path.with_extension("log.1"));
            }
            Some(OpenOptions::new().create(true).append(true).open(path)?)
        }
        None => None,
    };
    let stderr = o.stderr || file.is_none();
    tracing_subscriber::registry()
        .with(filter)
        .with(file.map(|f| fmt::layer().with_ansi(false).with_writer(Mutex::new(f))))
        .with(stderr.then(|| fmt::layer().with_writer(std::io::stderr)))
        .try_init()?;
    let _ = LEVEL.set(handle);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels() {
        assert_eq!(parse_level("Warning").unwrap(), LevelFilter::WARN);
        assert_eq!(parse_level("").unwrap(), LevelFilter::INFO);
        assert!(parse_level("loud").is_err());
        assert!(set_level("loud").is_err());
    }
}
