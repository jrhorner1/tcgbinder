//! `tcgbinder --web`: the app as a web application, the same UI and API the desktop
//! app shows, served over HTTP.

use std::process::ExitCode;
use std::sync::Arc;

use axum::body::{Body, to_bytes};
use axum::extract::{Request, State};
use axum::response::Response;
use http::{StatusCode, header};

use tcgbinder_api::{Api, Client, MAX_BODY, error_response, is_local_host};

use crate::{APP_NAME, Options, default_root, open};

/// Serves the web app on `listen` until interrupted.
pub fn serve(opts: &Options, listen: &str, open_browser: bool) -> ExitCode {
    let Some(root) = opts.data_dir.clone().or_else(default_root) else {
        eprintln!("tcgbinder: cannot find a data folder; use --data-dir");
        return ExitCode::FAILURE;
    };
    let app = match open(opts, root, "web", true) {
        Ok(app) => app,
        Err(e) => {
            eprintln!("tcgbinder: {e}");
            return ExitCode::FAILURE;
        }
    };
    let api = Arc::new(app.api(Client::Web, None));
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("tcgbinder: {e}");
            return ExitCode::FAILURE;
        }
    };
    let code = runtime.block_on(run(api, listen, open_browser));
    drop(app);
    code
}

async fn run(api: Arc<Api>, listen: &str, open_browser: bool) -> ExitCode {
    let listener = match tokio::net::TcpListener::bind(listen).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(addr = listen, error = %e, "cannot listen");
            eprintln!("tcgbinder: cannot listen on {listen}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let addr = listener.local_addr().expect("a bound address");
    let loopback = addr.ip().is_loopback();
    if !loopback {
        tracing::warn!(addr = %addr, "the web app is reachable from other computers and has no login: anyone who can reach it can view and change your collection");
    }
    let url = if loopback { format!("http://localhost:{}/", addr.port()) } else { format!("http://{addr}/") };
    tracing::info!(url, "web app ready");
    eprintln!("{APP_NAME} is running at {url} (Ctrl-C to stop)");
    if open_browser {
        open_url(&url);
    }
    let router = axum::Router::new().fallback(handle).with_state((api, loopback));
    let server = axum::serve(listener, router).with_graceful_shutdown(async {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("shutting down");
    });
    match server.await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(error = %e, "web server stopped");
            ExitCode::FAILURE
        }
    }
}

/// Answers every request through the API. On a loopback address only requests that
/// name localhost or an IP address are answered, which defeats DNS rebinding.
async fn handle(State((api, loopback)): State<(Arc<Api>, bool)>, req: Request) -> Response {
    let host = req.headers().get(header::HOST).and_then(|h| h.to_str().ok()).unwrap_or("").to_string();
    if loopback && !is_local_host(&host) {
        return into_axum(error_response(StatusCode::FORBIDDEN, &format!("unexpected host {host:?}")));
    }
    let (parts, body) = req.into_parts();
    let body = match to_bytes(body, MAX_BODY).await {
        Ok(b) => b.to_vec(),
        Err(_) => return into_axum(error_response(StatusCode::PAYLOAD_TOO_LARGE, "the request is too large")),
    };
    let req = http::Request::from_parts(parts, body);
    // The API blocks on files and the network.
    match tokio::task::spawn_blocking(move || api.handle(&req)).await {
        Ok(resp) => into_axum(resp),
        Err(e) => into_axum(error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string())),
    }
}

fn into_axum(resp: tcgbinder_api::Response) -> Response {
    resp.map(Body::from)
}

/// Opens `url` in the default browser.
fn open_url(url: &str) {
    let mut cmd = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(windows) {
        let mut c = std::process::Command::new("rundll32");
        c.arg("url.dll,FileProtocolHandler");
        c
    } else {
        std::process::Command::new("xdg-open")
    };
    if let Err(e) = cmd.arg(url).spawn() {
        tracing::warn!(error = %e, "could not open the browser");
    }
}
