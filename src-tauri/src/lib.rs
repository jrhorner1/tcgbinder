//! The TCG Binder app: the Tauri desktop and iPhone app, and (on desktop) the `--web`
//! server. Every front end runs the same UI (the `ui` folder) against the same API
//! (`tcgbinder-api`): the app serves both through its own `tcgb://` URL scheme, and
//! `--web` serves them over HTTP.

#[cfg(desktop)]
pub mod web;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

use tcgbinder_api::{Api, Client, Folders};
use tcgbinder_core::game::Game;
use tcgbinder_core::service::GameService;
use tcgbinder_core::settings::{self, SettingsStore};
use tcgbinder_core::{datadir, datafolder, instance, logging, version};

/// The app's display name.
pub const APP_NAME: &str = "TCG Binder";

/// The log file in the local folder.
pub const LOG_FILE: &str = "tcgbinder.log";

/// The URL scheme the app serves its UI and API on.
const SCHEME: &str = "tcgb";

/// The games the app manages. Each is its own crate; add new games here.
pub fn games() -> Vec<Box<dyn Game>> {
    vec![Box::new(tcgbinder_mtg::Magic::default())]
}

/// How the app was started.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// The data folder (`--data-dir` or `TCGBINDER_DATA_DIR`); when `None`, the
    /// `data_dir` setting, else the local folder.
    pub data_dir: Option<PathBuf>,
    /// The log file; `None` for the data folder's, "-" for stderr only.
    pub log_file: Option<String>,
    /// Overrides the Log detail setting.
    pub log_level: Option<String>,
}

/// Everything a running app holds: settings, the open games and the API over them.
pub struct App {
    /// Where the data lives (see `tcgbinder_core::datadir`).
    pub folders: Folders,
    pub settings: Arc<SettingsStore>,
    pub games: Vec<Arc<GameService>>,
    /// The single-instance lock (desktop), held while the app runs.
    _lock: Option<instance::Lock>,
}

/// Why the app couldn't open its data.
#[derive(Debug)]
pub enum StartError {
    /// Another copy is using the data folder.
    Running(instance::LockError, PathBuf),
    Other(anyhow::Error),
}

impl std::fmt::Display for StartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StartError::Running(e, root) => write!(f, "{e} (using {})", root.display()),
            StartError::Other(e) => write!(f, "{e:#}"),
        }
    }
}

impl From<anyhow::Error> for StartError {
    fn from(e: anyhow::Error) -> Self {
        StartError::Other(e)
    }
}

/// The data folder to use: the command line's, else the `data_dir` setting, else the
/// local folder. Returns it and whether the command line fixed it.
fn choose_data_folder(opts: &Options, local: &Path, settings: &SettingsStore) -> Result<(PathBuf, bool)> {
    if let Some(d) = &opts.data_dir {
        std::fs::create_dir_all(d).with_context(|| format!("cannot create the data folder {}", d.display()))?;
        return Ok((std::path::absolute(d).unwrap_or_else(|_| d.clone()), true));
    }
    let chosen = settings.get().data_dir;
    if chosen.is_empty() {
        return Ok((local.to_path_buf(), false));
    }
    let dir = PathBuf::from(&chosen);
    if !dir.is_dir() {
        // Don't make an empty one: the folder may be on a drive that isn't connected, or
        // not synced yet, and an empty collection there would be synced over the real one.
        anyhow::bail!(
            "the data folder {chosen} can't be found. Connect the drive it's on (or let your sync service download it) and open \
             {APP_NAME} again. To go back to the default folder, delete the \"data_dir\" line from {}.",
            local.join(settings::FILE_NAME).display()
        );
    }
    Ok((dir, false))
}

/// Opens the app's folders: the local folder (settings, logging, the single-instance
/// lock when `lock`), then every game's collection in the data folder, with their
/// daily price sync running in the background.
pub fn open(opts: &Options, local: PathBuf, mode: &str, lock: bool) -> Result<App, StartError> {
    std::fs::create_dir_all(&local).with_context(|| format!("cannot create the app's folder {}", local.display()))?;
    let local = std::path::absolute(&local).unwrap_or(local);
    let settings_file = local.join(settings::FILE_NAME);
    let had_settings = settings_file.exists();
    // Settings used to live in the data folder: bring those of a --data-dir folder in once.
    if let Some(old) = opts.data_dir.as_ref().map(|d| d.join(settings::FILE_NAME)).filter(|p| !had_settings && p.is_file()) {
        let _ = std::fs::copy(old, &settings_file);
    }
    let settings = Arc::new(SettingsStore::open(&local));
    let level = opts.log_level.clone().unwrap_or_else(|| settings.get().log_level);
    let log_file = match opts.log_file.as_deref() {
        None => Some(local.join(LOG_FILE)),
        Some("-") => None,
        Some(p) => Some(PathBuf::from(p)),
    };
    logging::setup(&logging::Options { file: log_file.clone(), level, stderr: cfg!(desktop) }).context("cannot open the log file")?;
    if opts.log_level.is_none() {
        settings.on_change(|old, new| {
            if old.log_level != new.log_level {
                let _ = logging::set_level(&new.log_level);
            }
        });
    }
    let (data, fixed) = choose_data_folder(opts, &local, &settings)?;
    let log = log_file.map(|p| p.display().to_string()).unwrap_or_else(|| "stderr".into());
    tracing::info!(version = version::describe("tcgbinder"), mode, local = %local.display(), data = %data.display(), log, "starting");

    // Two copies writing the same files would overwrite each other's changes.
    let held = if lock {
        match instance::acquire(&datadir::lock_path(&local, &data)) {
            Ok(l) => Some(l),
            Err(e @ instance::LockError::Running(_)) => {
                tracing::warn!(error = %e, "already running; exiting");
                return Err(StartError::Running(e, data));
            }
            Err(e) => {
                tracing::warn!(error = %e, "could not check for another running copy");
                None
            }
        }
    } else {
        None
    };

    let games = games();
    let first_run = !had_settings && !games.iter().any(|g| data.join(g.id()).join(datadir::COLLECTION_FILE).exists());
    let mut services = Vec::new();
    for g in games {
        let name = g.name();
        let mut svc = GameService::open(&local, &data, g).with_context(|| format!("cannot open the {name} collection"))?;
        svc.settings = Some(settings.clone());
        svc.auto_history = cfg!(desktop); // phones ask first (Home → Load price history)
        let copies = datafolder::conflict_copies(svc.store.file_path().parent().unwrap_or(&data));
        if !copies.is_empty() {
            tracing::warn!(game = svc.game.id(), ?copies, "another device saved a different version of the collection");
        }
        let svc = Arc::new(svc);
        svc.start_background_sync();
        services.push(svc);
    }
    let folders = Folders { local, data, fixed, first_run };
    Ok(App { folders, settings, games: services, _lock: held })
}

impl App {
    /// The API over this app's games for a front end.
    pub fn api(&self, client: Client, export_dir: Option<PathBuf>) -> Api {
        let mut api =
            Api::new(APP_NAME, version::VERSION, client, self.settings.clone(), self.games.clone()).with_folders(self.folders.clone());
        if tcgbinder_vision::available() {
            api = api.with_reader(tcgbinder_vision::read_image);
        }
        if let Some(dir) = export_dir {
            api = api.with_export_dir(dir);
        }
        api
    }
}

/// What the app's URL scheme serves: the API, or (when the data couldn't be opened) a
/// page saying why.
enum Served {
    Api(Arc<Api>),
    Failed(String),
}

/// The page shown when the app can't start.
fn failure_page(message: &str) -> tcgbinder_api::Response {
    let escaped = message.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let html = format!(
        "<!doctype html><meta charset=utf-8><meta name=viewport content='width=device-width'><title>{APP_NAME}</title>\
         <body style='font:16px system-ui;margin:0;display:grid;place-items:center;height:100vh;text-align:center'>\
         <div><h1 style='font-size:20px'>{APP_NAME} can't start</h1><p>{escaped}</p></div>"
    );
    http::Response::builder().header("Content-Type", "text/html; charset=utf-8").body(html.into_bytes()).unwrap()
}

/// Runs the app's window until it is closed.
pub fn run(opts: Options) {
    let builder = tauri::Builder::default();
    // Opening the app again shows the window that's already open.
    #[cfg(desktop)]
    let builder = builder
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init());
    builder
        .plugin(tauri_plugin_notification::init())
        .register_asynchronous_uri_scheme_protocol(SCHEME, |ctx, request, responder| {
            let served = ctx.app_handle().state::<Served>();
            match served.inner() {
                Served::Api(api) => {
                    let api = api.clone();
                    // API calls block on files and the network: answer off the UI thread.
                    std::thread::spawn(move || responder.respond(api.handle(&request)));
                }
                Served::Failed(msg) => responder.respond(failure_page(msg)),
            }
        })
        .setup(move |app| {
            let paths = app.path();
            let local = paths.app_data_dir()?;
            let (client, export_dir) = if cfg!(mobile) {
                // Documents shows in the Files app (Info.ios.plist).
                (Client::Phone, paths.document_dir().ok())
            } else {
                (Client::Desktop, paths.download_dir().ok())
            };
            let served = match open(&opts, local, if cfg!(mobile) { "phone" } else { "desktop" }, cfg!(desktop)) {
                Ok(started) => {
                    for g in &started.games {
                        g.set_alert_hook(alert_notifier(app.handle().clone()));
                    }
                    let api = started.api(client, export_dir);
                    #[cfg(desktop)]
                    let api = desktop_hooks(api, app.handle().clone());
                    let api = Arc::new(api);
                    app.manage(started);
                    Served::Api(api)
                }
                Err(e) => {
                    eprintln!("tcgbinder: {e}");
                    Served::Failed(match e {
                        StartError::Running(..) => format!("{APP_NAME} is already running. Switch to the open window to keep working."),
                        other => other.to_string(),
                    })
                }
            };
            app.manage(served);
            let url = if cfg!(any(windows, target_os = "android")) {
                format!("http://{SCHEME}.localhost/")
            } else {
                format!("{SCHEME}://localhost/")
            };
            let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::CustomProtocol(url.parse()?));
            #[cfg(desktop)]
            let window = window.title(APP_NAME).inner_size(1200.0, 800.0).min_inner_size(380.0, 500.0);
            window.build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running TCG Binder");
    tracing::info!("exiting");
}

/// Posts a price alert as a system notification.
fn alert_notifier(app: tauri::AppHandle) -> tcgbinder_core::service::AlertHook {
    use tauri_plugin_notification::NotificationExt;
    Box::new(move |_game, w| {
        let price = |v: f64| format!("${v:.2}");
        let now = w.price().map(price).unwrap_or_default();
        let was = if w.baseline > 0.0 { format!(", from {} on {}", price(w.baseline), w.baseline_date) } else { String::new() };
        let body = format!("{} ({}) is now {now}{was}.", w.card.name, w.finish);
        if let Err(e) = app.notification().builder().title("Price alert").body(body).show() {
            tracing::warn!(error = %e, "could not post the price alert");
        }
    })
}

/// The desktop app's folder picker and restart, for Settings → Data.
#[cfg(desktop)]
fn desktop_hooks(api: Api, app: tauri::AppHandle) -> Api {
    use tauri_plugin_dialog::DialogExt;
    let picker = app.clone();
    api.with_folder_picker(Arc::new(move || {
        let chosen = picker.dialog().file().set_title("Choose a folder for your collection").blocking_pick_folder()?;
        chosen.into_path().ok()
    }))
    .with_restart(Arc::new(move || app.restart()))
}

/// The iPhone app's entry point.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn mobile_main() {
    run(Options::default());
}
