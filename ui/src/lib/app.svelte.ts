// The UI's shared state and the calls every page uses: the app's info and settings,
// the game shown, its filter choices and lots, the status line, and talking to the
// app's JSON API (docs/web-api.md).
//
// `app` is reactive ($state): a page that reads app.filters redraws when it changes.

import type { Filters, Info, Job, Lot, Settings, Undo } from "./types";
import { plural } from "./format";

/** What Undo does: restore the backup the server took, or put back what the UI
 *  changed itself. */
export type UndoAction = Undo | (() => Promise<void>);

interface Status { message: string; error: boolean; undo: UndoAction | null; progress: Job | null }

export const app = $state({
  info: null as Info | null,
  settings: null as Settings | null,
  game: "",
  filters: null as Filters | null,
  lots: [] as Lot[],
  status: { message: "", error: false, undo: null, progress: null } as Status,
  /** The data's revision when the page was drawn: a change elsewhere redraws it. */
  revision: null as number | null,
  /** Our own change is coming: don't redraw for it. */
  skipRevision: false,
  /** Bumped to make the page shown load its data again. */
  reloads: 0,
});

/** The info, settings and filters, once loaded (pages only draw after that). */
export const info = () => app.info as Info;
export const settings = () => app.settings as Settings;
export const filters = () => app.filters as Filters;

export class ApiError extends Error {
  constructor(message: string, readonly status: number) { super(message); }
}

/** Calls the app's JSON API. Bodies may be objects (sent as JSON) or files and blobs
 *  (sent as their bytes). */
export async function api<T = any>(method: string, path: string, body?: unknown): Promise<T> {
  const headers: Record<string, string> = { "X-TCGBinder-Client": "ui" };
  let sent: BodyInit | undefined;
  // Blobs and files go as bytes: WebKit drops Blob bodies on the app's tcgb:// scheme.
  if (body instanceof Blob) sent = await body.arrayBuffer();
  else if (body instanceof ArrayBuffer) sent = body;
  else if (body !== undefined) { sent = JSON.stringify(body); headers["Content-Type"] = "application/json"; }
  if (method !== "GET") app.skipRevision = true; // our own change: no refresh needed
  const res = await fetch(path, { method, headers, body: sent });
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new ApiError(data.error || res.statusText, res.status);
  return data as T;
}

/** A path under the game shown, e.g. gameAPI("/cards") → /api/games/mtg/cards. */
export const gameAPI = (path: string) => `/api/games/${encodeURIComponent(app.game)}${path}`;

/** Shows a message in the status line, with Undo when the server backed up first. */
export function status(message: string, error = false, undo: UndoAction | null = null) {
  app.status = { message, error, undo, progress: null };
}
export const failed = (e: unknown) => status(e instanceof Error ? e.message : String(e), true);

/** Redraws the page shown with fresh data. */
export const reload = () => { app.reloads++; };

/** Undoes a change the server took a backup before, unless the collection has
 *  changed since (the server checks the revision). */
export async function undoChange(undo: UndoAction) {
  try {
    if (typeof undo === "function") {
      await undo();
      status("Undone.");
      reload();
      return;
    }
    const r = await api("POST", gameAPI("/backups/restore"), { name: undo.backup, ifRevision: undo.revision });
    status(`Undone: the collection is back to ${plural(r.cards, "unique card", "unique cards")}.`);
    await loadFilters();
    reload();
  } catch (e) { failed(e); }
}

/** Shows a long job's progress in the status line (asking GET /jobs each second)
 *  until `promise` settles, and returns its result. */
export async function whileRunning<T>(promise: Promise<T>): Promise<T> {
  let running = true;
  const timer = setInterval(async () => {
    try {
      const r = await api<{ jobs: Job[] }>("GET", gameAPI("/jobs"));
      const j = r.jobs[0];
      if (!running || !j || !j.message) return;
      const count = j.total ? ` — ${j.done.toLocaleString()} of ${j.total.toLocaleString()}` : "";
      app.status = { message: `${j.message}${count}…`, error: false, undo: null, progress: j };
    } catch { /* the job's own answer reports failures */ }
  }, 1000);
  try { return await promise; } finally { running = false; clearInterval(timer); }
}

/** Runs a long job (refresh prices, load price history), saying what it did. */
export async function runJob(path: string, busy: string, done: (r: any) => string) {
  status(busy);
  try {
    const r = await whileRunning(api("POST", gameAPI(path)));
    status(done(r));
    reload();
  } catch (e) { failed(e); }
}

export async function loadFilters() {
  app.filters = await api<Filters>("GET", gameAPI("/filters"));
}

/** Loads the game's lots into app.lots (for their names, and the Add dialog). */
export async function loadLots(): Promise<Lot[]> {
  try { app.lots = await api<Lot[]>("GET", gameAPI("/lots")); } catch { /* keep the last */ }
  return app.lots;
}

/** Sends changed settings, then applies what was saved. */
export async function saveSettings(change: Partial<Settings>, what: string) {
  try {
    const r = await api<{ settings: Settings }>("PUT", "/api/settings", change);
    app.settings = r.settings;
    applySettings();
    status(`Saved: ${what}`);
  } catch (e) { failed(e); }
}

/** Applies the appearance settings to the page. */
export function applySettings() {
  const s = settings();
  const root = document.documentElement;
  if (s.theme === "system") delete root.dataset.theme; else root.dataset.theme = s.theme;
  root.style.setProperty("--scale", String({ small: 0.9, normal: 1, large: 1.15, larger: 1.3 }[s.text_size] || 1));
}

/** Saves a small preference in the browser (the last list used, the game shown). */
export function store(key: string, value?: string): string | null {
  try {
    if (value === undefined) return localStorage.getItem(key);
    localStorage.setItem(key, value);
  } catch { /* private mode */ }
  return null;
}

/** Downloads an export, or saves it where the app can't download (desktop, iPhone):
 *  format json (the collection file) or csv, for the cards `params` filters (the
 *  collection list's), or every card. */
export async function exportCollection(format: "json" | "csv" | "report" = "json", params?: URLSearchParams) {
  const qs = new URLSearchParams(params || []);
  for (const k of ["key", "view"]) qs.delete(k);
  qs.set("format", format);
  if (!info().saveExports) { location.href = gameAPI(`/export?${qs}`); return; }
  const what = { json: "the collection", csv: "the CSV", report: "the valuation report" }[format];
  try {
    const r = await api("POST", gameAPI(`/export?${qs}`));
    status(`Saved ${what} to ${r.path}`);
  } catch (e) { failed(e); }
}
