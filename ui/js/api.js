// Talking to the app's JSON API (docs/web-api.md): api() and gameAPI(), undo, long
// jobs with their progress, and loading the filter choices and lots other files use.

import { applySettings, status, whileRunning } from "./dom.js";
import { plural } from "./format.js";
import { route } from "./nav.js";
import { state } from "./state.js";

// undoChange restores the backup taken before a change, unless the collection has
// changed since (the server checks the revision).
export async function undoChange(undo) {
  try {
    const r = await api("POST", gameAPI("/backups/restore"), { name: undo.backup, ifRevision: undo.revision });
    status(`Undone: the collection is back to ${plural(r.cards, "unique card", "unique cards")}.`);
    await loadFilters();
    route();
  } catch (e) { status(e.message, true); }
}

// api calls the app's JSON API. Bodies may be objects (sent as JSON) or files and
// blobs (sent as they are).
export async function api(method, path, body) {
  const opts = { method, headers: { "X-TCGBinder-Client": "ui" } };
  // Blobs and files go as bytes: WebKit drops Blob bodies on the app's tcgb:// scheme.
  if (body instanceof Blob) opts.body = await body.arrayBuffer();
  else if (body instanceof ArrayBuffer) opts.body = body;
  else if (body !== undefined) {
    opts.body = JSON.stringify(body);
    opts.headers["Content-Type"] = "application/json";
  }
  if (method !== "GET") state.skipRevision = true; // our own change: no refresh needed
  const res = await fetch(path, opts);
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(data.error || res.statusText);
  return data;
}

export const gameAPI = (path) => `/api/games/${encodeURIComponent(state.game)}${path}`;

// dataFolder is where the collection is kept (GET /api/data-folder), or null where
// it can't be changed.
export const dataFolder = () => api("GET", "/api/data-folder").catch(() => null);

export async function runJob(button, path, busy, done) {
  button.disabled = true;
  status(busy);
  try {
    const r = await whileRunning(api("POST", gameAPI(path)));
    status(done(r));
    route();
  } catch (err) {
    status(err.message, true);
  } finally {
    button.disabled = false;
  }
}

// loadLots keeps the game's lots in state.lots (for names and the Add page).
export async function loadLots() {
  try { state.lots = await api("GET", gameAPI("/lots")); } catch { state.lots = state.lots || []; }
  return state.lots;
}

// exportCollection downloads (or, in apps that can't download, saves) an export:
// format "json" (the collection file), "csv" or "report", for the cards `params`
// filters (the collection list's parameters), or every card.
export async function exportCollection(button, format = "json", params) {
  const qs = new URLSearchParams(params || []);
  for (const k of ["key", "view"]) qs.delete(k);
  qs.set("format", format);
  if (!state.info.saveExports) { location.href = gameAPI(`/export?${qs}`); return; }
  const what = { json: "the collection", csv: "the CSV", report: "the valuation report" }[format];
  if (button) button.disabled = true;
  try {
    const r = await api("POST", gameAPI(`/export?${qs}`));
    status(`Saved ${what} to ${r.path}`);
  } catch (e) { status(e.message, true); }
  if (button) button.disabled = false;
}

// saveSettings sends changed settings, then applies what was saved.
export async function saveSettings(change, what) {
  try {
    const r = await api("PUT", "/api/settings", change);
    state.settings = r.settings;
    applySettings();
    status(`Saved: ${what}`);
  } catch (e) { status(e.message, true); }
}

export async function loadFilters() {
  state.filters = await api("GET", gameAPI("/filters"));
}
