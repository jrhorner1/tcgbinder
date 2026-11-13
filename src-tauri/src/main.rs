// TCG Binder: by default it opens the desktop app; with --web it serves the same
// collection as a web application.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

use tcgbinder_lib::{Options, web};

/// Keeps the web app private to this computer unless asked otherwise.
const DEFAULT_LISTEN: &str = "127.0.0.1:8080";

/// TCG Binder — manage your trading card collection.
#[derive(Parser, Debug)]
#[command(
    name = "tcgbinder",
    version = tcgbinder_core::version::VERSION,
    after_help = "Examples:\n  tcgbinder --web --open\n  tcgbinder --web --listen 0.0.0.0:8080 --log-level debug\n  tcgbinder --data-dir ~/cards --log-file ~/cards/app.log"
)]
struct Cli {
    /// Folder for collections and caches (default: the app's data folder)
    #[arg(long, value_name = "FOLDER", env = "TCGBINDER_DATA_DIR")]
    data_dir: Option<PathBuf>,
    /// Write logs to PATH (default: tcgbinder.log in the data folder; "-" for stderr only)
    #[arg(long, value_name = "PATH")]
    log_file: Option<String>,
    /// Minimum log level: debug, info, warn or error (default: the Log detail setting)
    #[arg(long, value_name = "LEVEL", value_parser = parse_level)]
    log_level: Option<String>,
    /// Serve the app as a web application instead of opening the desktop window
    #[arg(long)]
    web: bool,
    /// Address for --web; use 0.0.0.0:8080 to allow other devices (no login!)
    #[arg(long, value_name = "ADDRESS", default_value = DEFAULT_LISTEN)]
    listen: String,
    /// With --web, open the app in the default browser
    #[arg(long)]
    open: bool,
}

fn parse_level(s: &str) -> Result<String, String> {
    tcgbinder_core::logging::parse_level(s).map(|_| s.to_string()).map_err(|e| e.to_string())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let opts = Options { data_dir: cli.data_dir, log_file: cli.log_file, log_level: cli.log_level };
    if cli.web {
        return web::serve(&opts, &cli.listen, cli.open);
    }
    tcgbinder_lib::run(opts);
    ExitCode::SUCCESS
}
