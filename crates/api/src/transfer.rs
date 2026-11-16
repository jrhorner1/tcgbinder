//! Getting cards in and out: import (with a preview to confirm), export, backups and
//! restoring one, and the long jobs (import, price refresh, price history) with their
//! progress.

use crate::{Api, Error, Reply, Request, Result, bad, decode, internal, lots, not_found, report, tags_from, undo, upstream};
use http::{StatusCode, header};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tcgbinder_core::collection::{Card, normalize_tags, round_cents, write_csv};
use tcgbinder_core::dates;
use tcgbinder_core::import::{ImportPreview, ImportResult};
use tcgbinder_core::service::GameService;

/// How far a long job has got.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct JobProgress {
    /// Steps done and in all (both 0 when the job can't count them).
    pub done: usize,
    pub total: usize,
    /// What it's doing now, e.g. "Downloading price history".
    pub message: String,
}

/// Lets a job report its progress, for `GET .../jobs`.
pub struct Reporter<'a> {
    api: &'a Api,
    key: String,
}

impl Reporter<'_> {
    /// Records how far the job has got.
    pub fn report(&self, done: usize, total: usize, message: &str) {
        if let Some(p) = self.api.busy.lock().unwrap().get_mut(&self.key) {
            *p = JobProgress { done, total, message: message.to_string() };
        }
    }
}

/// How long an import preview can wait to be committed.
const PREVIEW_LIFETIME: Duration = Duration::from_secs(15 * 60);

/// An import preview kept until it is committed, replaced or too old.
pub(crate) struct PendingImport {
    token: String,
    made: Instant,
    preview: ImportPreview,
}

/// A token no other preview in this process has had.
fn new_token() -> String {
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    format!("{nanos:x}-{:x}", COUNT.fetch_add(1, Ordering::Relaxed))
}

/// What an import did, as the API answers it.
fn import_result(g: &GameService, res: &ImportResult) -> Value {
    json!({
        "summary": res.summary(), "cards": res.cards, "newCards": res.new_cards,
        "alreadyOwned": res.already_owned, "format": res.report.format, "undo": undo(g, &res.backup),
    })
}

/// Adds `tags` to every purchase of `cards` (an import's "tag every card").
fn tag_every_purchase(cards: &mut [Card], tags: &[String]) -> anyhow::Result<()> {
    if tags.is_empty() {
        return Ok(());
    }
    for c in cards {
        for list in c.copies.0.values_mut().flat_map(|m| m.values_mut()) {
            for p in list {
                let mut all = std::mem::take(&mut p.tags);
                all.extend(tags.iter().cloned());
                p.tags = normalize_tags(&all).map_err(|e| e.context(format!("{} ({} · {})", c.name, c.set, c.collector_number)))?;
            }
        }
    }
    Ok(())
}

/// The contents of a multipart upload's "file" field.
fn multipart_file(content_type: &str, body: &[u8]) -> Option<Vec<u8>> {
    let boundary = content_type.split(';').find_map(|p| p.trim().strip_prefix("boundary="))?.trim_matches('"');
    let delim = format!("--{boundary}").into_bytes();
    let find = |hay: &[u8], needle: &[u8], from: usize| hay[from..].windows(needle.len()).position(|w| w == needle).map(|i| i + from);
    let mut at = find(body, &delim, 0)?;
    loop {
        let start = at + delim.len();
        let next = find(body, &delim, start)?;
        let part = &body[start..next];
        let header_end = find(part, b"\r\n\r\n", 0)?;
        let headers = String::from_utf8_lossy(&part[..header_end]).to_lowercase();
        if headers.contains("name=\"file\"") {
            let content = &part[header_end + 4..];
            return Some(content.strip_suffix(b"\r\n").unwrap_or(content).to_vec());
        }
        at = next;
    }
}

impl Api {
    /// Runs a long operation unless one of the same kind is already running. `run` can
    /// report its progress, which `GET .../jobs` gives while it runs.
    pub(crate) fn job(&self, g: &GameService, name: &str, run: impl FnOnce(&Reporter) -> anyhow::Result<Value>) -> Result {
        let key = format!("{}/{name}", g.game.id());
        {
            let mut busy = self.busy.lock().unwrap();
            if busy.contains_key(&key) {
                return Err(Error { status: StatusCode::CONFLICT, message: "already running".into() });
            }
            busy.insert(key.clone(), JobProgress::default());
        }
        let res = run(&Reporter { api: self, key: key.clone() });
        self.busy.lock().unwrap().remove(&key);
        res.map(Reply::Json).map_err(upstream)
    }

    /// The game's long jobs running now, with their progress.
    pub(crate) fn jobs(&self, g: &GameService) -> Reply {
        let prefix = format!("{}/", g.game.id());
        let busy = self.busy.lock().unwrap();
        let jobs: Vec<Value> = busy
            .iter()
            .filter_map(|(k, p)| {
                k.strip_prefix(&prefix).map(|name| json!({ "name": name, "done": p.done, "total": p.total, "message": p.message }))
            })
            .collect();
        Reply::Json(json!({ "jobs": jobs }))
    }

    /// Replaces the collection with a backup. With `ifRevision` (Undo), only while the
    /// collection is still at that revision: otherwise something changed since, and
    /// the undo would lose it.
    pub(crate) fn restore_backup(&self, g: &GameService, body: &[u8]) -> Result {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Req {
            name: String,
            if_revision: Option<u64>,
        }
        let req: Req = decode(body)?;
        if req.if_revision.is_some_and(|r| r != g.store.revision()) {
            return Err(Error {
                status: StatusCode::CONFLICT,
                message: "the collection has changed since, so it can't be undone here; use Tools → Restore a backup".into(),
            });
        }
        if !g.store.backups().map_err(internal)?.iter().any(|b| b.name == req.name) {
            return Err(not_found(format!("there is no backup named {:?}", req.name)));
        }
        let backup = g.store.restore(&req.name).map_err(internal)?;
        Ok(Reply::Json(json!({ "cards": g.store.len(), "undo": undo(g, &backup) })))
    }

    /// The file sent as a multipart "file" field or as the raw body.
    pub(crate) fn upload(req: &Request) -> Result<Vec<u8>> {
        let content_type = req.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("");
        if content_type.starts_with("multipart/") {
            multipart_file(content_type, req.body()).ok_or_else(|| bad("no \"file\" in the upload"))
        } else {
            Ok(req.body().clone())
        }
    }

    /// Imports the file sent as a multipart "file" field or as the raw body.
    pub(crate) fn import(&self, g: &GameService, req: &Request) -> Result {
        let data = Self::upload(req)?;
        self.job(g, "import", |r| Ok(import_result(g, &g.import(&data, &mut |done, total| r.report(done, total, "Matching cards"))?)))
    }

    /// Reads and matches an import file without saving it, and keeps the result for
    /// `import/commit`. A new preview replaces the game's previous one.
    pub(crate) fn preview_import(&self, g: &GameService, req: &Request) -> Result {
        let data = Self::upload(req)?;
        self.job(g, "import", |r| {
            let preview = g.preview_import(&data, &mut |done, total| r.report(done, total, "Matching cards"))?;
            let token = new_token();
            let answer = json!({
                "token": token, "format": preview.report.format, "rows": preview.report.rows,
                "cards": preview.printings(), "copies": preview.copies, "newCards": preview.new_cards,
                "alreadyOwned": preview.already_owned, "pricePaid": round_cents(preview.price_paid),
                "notes": preview.report.notes, "unmatched": preview.report.unmatched, "header": preview.report.header,
            });
            let pending = PendingImport { token, made: Instant::now(), preview };
            self.previews.lock().unwrap().insert(g.game.id().to_string(), pending);
            Ok(answer)
        })
    }

    /// Saves the game's import preview, if `token` names it and it isn't too old.
    pub(crate) fn commit_import(&self, g: &GameService, body: &[u8]) -> Result {
        #[derive(Deserialize)]
        struct Req {
            token: String,
            /// Import the cards into this lot (a pasted list or a CSV of its contents).
            #[serde(default)]
            lot: String,
            /// Tags added to every purchase in the import.
            #[serde(default)]
            tags: Vec<String>,
        }
        let req: Req = decode(body)?;
        let tags = tags_from(&req.tags)?;
        let lot = match req.lot.as_str() {
            "" => None,
            id => Some(self.open_lot(g, id)?),
        };
        let pending = {
            let mut previews = self.previews.lock().unwrap();
            previews.retain(|_, p| p.made.elapsed() < PREVIEW_LIFETIME);
            match previews.get(g.game.id()) {
                Some(p) if p.token == req.token => previews.remove(g.game.id()),
                _ => None,
            }
        };
        let pending = pending.ok_or_else(|| Error {
            status: StatusCode::GONE,
            message: "this preview is no longer available; preview the file again".into(),
        })?;
        let mut preview = pending.preview;
        if let Some(lot) = &lot {
            lots::put_in_lot(&mut preview.cards, lot);
        }
        tag_every_purchase(&mut preview.cards, &tags).map_err(|e| bad(format!("{e:#}")))?;
        self.job(g, "import", |r| {
            r.report(0, 0, "Saving");
            let res = g.commit_import(preview)?;
            let mut answer = import_result(g, &res);
            if let Some(lot) = &lot {
                g.reprice_lot(lot)?;
                answer["lot"] = lots::summary(g, lot);
            }
            Ok(answer)
        })
    }

    /// An export for `GET`/`POST /export`: `format=json` (the default) is the whole
    /// collection file, which import reads back; `csv` and `report` (a printable HTML
    /// valuation) take the same filter and sort parameters as `GET /cards`.
    pub(crate) fn export_file(&self, g: &GameService, params: &HashMap<String, String>) -> Result<(String, &'static str, Vec<u8>)> {
        let stem = format!("{}-collection-{}", g.game.id(), dates::today_string());
        match params.get("format").map_or("json", |f| f.as_str()) {
            "json" => {
                let data = std::fs::read(g.store.file_path()).map_err(|e| internal(e.into()))?;
                Ok((format!("{stem}.json"), "application/json", data))
            }
            "csv" => {
                let (cards, _, _) = self.filtered_cards(g, params)?;
                let data = write_csv(&cards, g.game.catalog_id_label()).map_err(internal)?;
                Ok((format!("{stem}.csv"), "text/csv; charset=utf-8", data))
            }
            "report" => {
                let (cards, _, _) = self.filtered_cards(g, params)?;
                let filtered = params.keys().any(|k| !matches!(k.as_str(), "format" | "sort" | "dir"));
                let html = report::valuation(g, &cards, filtered);
                Ok((format!("{}-valuation-{}.html", g.game.id(), dates::today_string()), "text/html; charset=utf-8", html.into_bytes()))
            }
            _ => Err(bad("format must be json, csv or report")),
        }
    }

    /// Downloads an export (see `export_file`).
    pub(crate) fn export(&self, g: &GameService, params: &HashMap<String, String>) -> Result {
        let (name, content_type, data) = self.export_file(g, params)?;
        Ok(Reply::File { name, content_type, data })
    }

    /// Saves an export (see `export_file`) to the export folder.
    pub(crate) fn save_export(&self, g: &GameService, params: &HashMap<String, String>) -> Result {
        let dir = self.export_dir.as_ref().ok_or_else(|| not_found("this app downloads exports instead (GET)"))?;
        let (name, _, data) = self.export_file(g, params)?;
        let path = dir.join(name);
        std::fs::create_dir_all(dir).and_then(|_| std::fs::write(&path, data)).map_err(|e| internal(e.into()))?;
        tracing::info!(game = g.game.id(), path = %path.display(), "collection exported");
        Ok(Reply::Json(json!({ "path": path.display().to_string() })))
    }
}
