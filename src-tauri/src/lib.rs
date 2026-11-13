//! The TCG Binder app: the Tauri desktop and iPhone app, and (on desktop) the `--web`
//! server. Every front end runs the same UI (the `ui` folder) against the same API
//! (`tcgbinder-api`): the app serves both through its own `tcgb://` URL scheme, and
//! `--web` serves them over HTTP.

#[cfg(desktop)]
pub mod web;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

use tcgbinder_api::{Api, Client};
use tcgbinder_core::game::Game;
use tcgbinder_core::service::GameService;
use tcgbinder_core::settings::SettingsStore;
use tcgbinder_core::{datadir, instance, logging, version};

/// The app's display name.
pub const APP_NAME: &str = "TCG Binder";

/// The log file in the data folder.
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
    /// The folder for collections and caches; the app's data folder if `None`.
    pub data_dir: Option<PathBuf>,
    /// The log file; `None` for the data folder's, "-" for stderr only.
    pub log_file: Option<String>,
    /// Overrides the Log detail setting.
    pub log_level: Option<String>,
}

/// Everything a running app holds: settings, the open games and the API over them.
pub struct App {
    pub root: PathBuf,
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

/// Opens the data folder: settings, logging, the single-instance lock (`lock`), and
/// every game's collection, with their daily price sync running in the background.
pub fn open(opts: &Options, root: PathBuf, mode: &str, lock: bool) -> Result<App, StartError> {
    std::fs::create_dir_all(&root).with_context(|| format!("cannot create the data folder {}", root.display()))?;
    let root = std::path::absolute(&root).unwrap_or(root);
    let settings = Arc::new(SettingsStore::open(&root));
    let level = opts.log_level.clone().unwrap_or_else(|| settings.get().log_level);
    let log_file = match opts.log_file.as_deref() {
        None => Some(root.join(LOG_FILE)),
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
    let log = log_file.map(|p| p.display().to_string()).unwrap_or_else(|| "stderr".into());
    tracing::info!(version = version::describe("tcgbinder"), mode, data = %root.display(), log, "starting");

    // Two copies writing the same files would overwrite each other's changes.
    let held = if lock {
        match instance::acquire(&root) {
            Ok(l) => Some(l),
            Err(e @ instance::LockError::Running(_)) => {
                tracing::warn!(error = %e, "already running; exiting");
                return Err(StartError::Running(e, root));
            }
            Err(e) => {
                tracing::warn!(error = %e, "could not check for another running copy");
                None
            }
        }
    } else {
        None
    };

    let mut services = Vec::new();
    for g in games() {
        let name = g.name();
        let mut svc = GameService::open(&root, g).with_context(|| format!("cannot open the {name} collection"))?;
        svc.settings = Some(settings.clone());
        svc.auto_history = cfg!(desktop); // phones ask first (Home → Load price history)
        let svc = Arc::new(svc);
        svc.start_background_sync();
        services.push(svc);
    }
    Ok(App { root, settings, games: services, _lock: held })
}

impl App {
    /// The API over this app's games for a front end.
    pub fn api(&self, client: Client, export_dir: Option<PathBuf>) -> Api {
        let mut api = Api::new(APP_NAME, version::VERSION, client, self.settings.clone(), self.games.clone());
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
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _, _| {
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.unminimize();
            let _ = w.set_focus();
        }
    }));
    builder
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
            let root = match &opts.data_dir {
                Some(d) => d.clone(),
                None => paths.app_data_dir()?,
            };
            let (client, export_dir) = if cfg!(mobile) {
                // Documents shows in the Files app (Info.ios.plist).
                (Client::Phone, paths.document_dir().ok())
            } else {
                (Client::Desktop, paths.download_dir().ok())
            };
            let served = match open(&opts, root, if cfg!(mobile) { "phone" } else { "desktop" }, cfg!(desktop)) {
                Ok(started) => {
                    let api = Arc::new(started.api(client, export_dir));
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

/// The iPhone app's entry point.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn mobile_main() {
    run(Options::default());
}

/// The desktop data folder, or `TCGBINDER_DATA_DIR` when set.
pub fn default_root() -> Option<PathBuf> {
    std::env::var_os("TCGBINDER_DATA_DIR").map(PathBuf::from).or_else(datadir::default_root)
}
