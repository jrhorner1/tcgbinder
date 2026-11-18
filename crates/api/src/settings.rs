//! The app's settings, and opening a link in the system's browser.

use crate::{Api, Error, Reply, Result, bad, decode, internal};
use http::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};
use tcgbinder_core::settings::{Settings, choices};

impl Api {
    pub(crate) fn get_settings(&self) -> Reply {
        let data_dir = match &self.folders {
            Some(f) => f.data.display().to_string(),
            None => self.settings.path().and_then(|p| p.parent()).map(|d| d.display().to_string()).unwrap_or_default(),
        };
        let choices: serde_json::Map<String, Value> = choices::ALL.iter().map(|(k, v)| (k.to_string(), json!(v))).collect();
        Reply::Json(json!({ "settings": self.settings.get(), "choices": choices, "defaults": Settings::default(), "dataDir": data_dir }))
    }

    /// Changes the settings sent; settings left out keep their values.
    pub(crate) fn put_settings(&self, body: &[u8]) -> Result {
        let change: Value = decode(body)?;
        let next = self.settings.get().with_changes(&change).map_err(|e| bad(e.to_string()))?;
        self.settings.set(next).map_err(|e| bad(e.to_string()))?;
        Ok(Reply::Json(json!({ "settings": self.settings.get() })))
    }

    /// Opens `{"url"}` (http or https only) in the system's browser.
    pub(crate) fn open_link(&self, body: &[u8]) -> Result {
        #[derive(Deserialize)]
        struct Req {
            url: String,
        }
        let req: Req = decode(body)?;
        let open = self
            .open_url
            .as_ref()
            .ok_or_else(|| Error { status: StatusCode::NOT_IMPLEMENTED, message: "links open in this browser here".into() })?;
        let url = url::Url::parse(req.url.trim()).map_err(|_| bad("that isn't a web address"))?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(bad("only http and https links can be opened"));
        }
        open(url.as_str()).map_err(|e| internal(e.context("opening the link")))?;
        tracing::info!(host = url.host_str().unwrap_or(""), "link opened in the browser");
        Ok(Reply::Json(json!({ "opened": true })))
    }
}
