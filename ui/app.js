// TCG Binder's user interface, shared by the desktop app, the iPhone app and the web
// app (--web). Plain JavaScript, no build step. All text from the API is inserted with
// textContent (via h()), never as HTML, so card data can't inject markup.
"use strict";

const state = {
  info: null, settings: null, game: null, filters: null, range: null,
  // Ticked cards for bulk edit, per game: key → {name, where}. Ticks stay while the
  // filters hide the cards, so a selection can be built from several searches.
  ticked: new Map(),
  revision: null, skipRevision: false,
  scanStop: null, // stops the camera when leaving the Scan page
};
const $ = (sel) => document.querySelector(sel);

// h builds an element: h("div.cls", {attr: v, onclick: fn}, child, "text", ...).
function h(spec, attrs, ...children) {
  const [tag, ...classes] = spec.split(".");
  const el = document.createElement(tag || "div");
  if (classes.length) el.className = classes.join(" ");
  if (attrs && (typeof attrs !== "object" || attrs instanceof Node || Array.isArray(attrs))) {
    children.unshift(attrs);
    attrs = null;
  }
  for (const [k, v] of Object.entries(attrs || {})) {
    if (v === undefined || v === null || v === false) continue;
    if (k.startsWith("on")) el.addEventListener(k.slice(2), v);
    else if (k === "style") el.style.cssText = v;
    else if (k === "checked" || k === "value") el[k] = v;
    else el.setAttribute(k, v === true ? "" : v);
  }
  for (const c of children.flat()) {
    if (c === null || c === undefined || c === false) continue;
    el.append(c instanceof Node ? c : document.createTextNode(String(c)));
  }
  return el;
}

// fill replaces el's children, flattening lists and skipping null/false like h().
function fill(el, ...children) {
  el.replaceChildren(...children.flat(Infinity).filter((c) => c !== null && c !== undefined && c !== false)
    .map((c) => (c instanceof Node ? c : document.createTextNode(String(c)))));
}

function store(key, value) {
  try {
    if (value === undefined) return localStorage.getItem(key);
    localStorage.setItem(key, value);
  } catch { return null; }
}

const usd = (v) => (v < 0 ? "-$" : "$") + Math.abs(v).toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 });
// sn shows where a card is printed, e.g. "M19 · 201".
const sn = (c) => (c.collector_number ? `${c.set} · ${c.collector_number}` : c.set);
const title = (s) => (s ? s[0].toUpperCase() + s.slice(1) : "");
// fname shows a finish as players say it, e.g. "nonfoil" → "Normal".
const fname = (f) => (state.filters && state.filters.finishNames && state.filters.finishNames[f]) || title(f || "");
const plural = (n, one, many) => `${n.toLocaleString()} ${n === 1 ? one : many}`;
// today is the local date as YYYY-MM-DD (toISOString would give the UTC date).
const today = () => { const d = new Date(); return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`; };
const hashParams = () => new URLSearchParams(location.hash.split("?")[1]);
const gameInfo = () => state.info.games.find((g) => g.ID === state.game) || state.info.games[0];
const shortName = (name) => name.split(":")[0];
const finishRank = (f) => { const i = state.filters.finishes.indexOf(f); return i < 0 ? 99 : i; };
const anyPrice = (c) => (c.each ? c.each : Object.values(c.market || {})[0] || 0);
// languageName shows a language code as its name, e.g. "ja" → "Japanese".
const languageName = (code) => ((state.filters && state.filters.languages) || []).find((l) => l.code === code)?.name || (code || "").toUpperCase();
// noted marks a card with a note on any of its purchases.
const noted = (c) => ((c.purchases || []).some((p) => p.note) ? "has a note" : null);
// englishPrice marks a card valued at its English printing's price (the catalog has
// none for its own language).
const englishPrice = (c) => (c.price_from === "en" ? "English price" : null);

// copiesSummary describes a card's copies compactly: "×3" when they're all in the usual
// finish and near mint, otherwise per finish, e.g. "Normal ×2 · Foil ×1 (1 LP)".
function copiesSummary(c) {
  const byFinish = new Map();
  for (const l of c.purchases || []) {
    const f = byFinish.get(l.finish) || { n: 0, worn: new Map() };
    f.n += l.quantity;
    if (l.condition !== "NM") f.worn.set(l.condition, (f.worn.get(l.condition) || 0) + l.quantity);
    byFinish.set(l.finish, f);
  }
  const finishes = [...byFinish.keys()].sort((a, b) => finishRank(a) - finishRank(b) || a.localeCompare(b));
  if (finishes.length === 1 && finishes[0] === state.filters.finishes[0] && !byFinish.get(finishes[0]).worn.size) return `×${c.copy_count}`;
  const conds = state.filters.conditions;
  return finishes.map((f) => {
    const x = byFinish.get(f);
    const worn = [...x.worn].sort((a, b) => conds.indexOf(a[0]) - conds.indexOf(b[0])).map(([k, n]) => `${n} ${k}`);
    return `${fname(f)} ×${x.n}${worn.length ? ` (${worn.join(", ")})` : ""}`;
  }).join(" · ");
}

// status shows a message in the status line, optionally with Undo for a change the
// server took a backup before (`undo` from its answer: {backup, revision}).
// status shows a message in the status line, optionally with Undo for a change the
// server took a backup before (`undo` from its answer: {backup, revision}), or a
// progress bar ({done, total}).
function status(msg, isError, undo, progress) {
  const el = $("#status");
  const undoBtn = undo && h("button.link", { type: "button", onclick: () => undoChange(undo) }, "Undo");
  const bar = progress && progress.total ? h("progress", { value: progress.done, max: progress.total }) : null;
  fill(el, bar, msg || "", undoBtn ? " " : null, undoBtn);
  el.className = isError ? "error" : "";
}

// whileRunning shows a long job's progress in the status line (polling GET /jobs
// once a second) until `promise` settles, and returns its result.
async function whileRunning(promise) {
  let running = true;
  const timer = setInterval(async () => {
    try {
      const r = await api("GET", gameAPI("/jobs"));
      const j = r.jobs[0];
      if (!running || !j || !j.message) return;
      const count = j.total ? ` — ${j.done.toLocaleString()} of ${j.total.toLocaleString()}` : "";
      status(`${j.message}${count}…`, false, null, j);
    } catch { /* the job's own answer reports failures */ }
  }, 1000);
  try { return await promise; } finally { running = false; clearInterval(timer); }
}

// undoChange restores the backup taken before a change, unless the collection has
// changed since (the server checks the revision).
async function undoChange(undo) {
  try {
    const r = await api("POST", gameAPI("/backups/restore"), { name: undo.backup, ifRevision: undo.revision });
    status(`Undone: the collection is back to ${plural(r.cards, "unique card", "unique cards")}.`);
    await loadFilters();
    route();
  } catch (e) { status(e.message, true); }
}

// api calls the app's JSON API. Bodies may be objects (sent as JSON) or files and
// blobs (sent as they are).
async function api(method, path, body) {
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
const gameAPI = (path) => `/api/games/${encodeURIComponent(state.game)}${path}`;

// confirmDialog asks a yes/no question in the app's own dialog and resolves to true
// for yes. (The apps' web views answer window.confirm() with "no" without showing it,
// so the browser's own dialog can't be used.)
function confirmDialog(message, { danger = false, action = "OK", title = "Are you sure?" } = {}) {
  return askDialog(title, message, [{ label: action, value: true, primary: !danger, danger }]).then((v) => v === true);
}

// ask confirms a deletion, unless the user turned that off in Settings.
const ask = async (msg, action = "Delete") => !state.settings.confirm_delete || confirmDialog(msg, { danger: true, action });

// askDialog asks one question in a dialog of its own (so it can open over another)
// and resolves to the chosen button's value, or null if closed. buttons: [{label,
// value, primary, danger}]; with cancel false there is no Cancel button (a message
// to acknowledge).
function askDialog(title, message, buttons, cancel = true) {
  return new Promise((resolve) => {
    const dlg = h("dialog.ask");
    let answered = false;
    const done = (v) => { if (answered) return; answered = true; dlg.close(); dlg.remove(); resolve(v); };
    const choices = buttons.map((b) => h("button" + (b.primary ? ".primary" : b.danger ? ".danger" : ""), { type: "button", onclick: () => done(b.value) }, b.label));
    fill(dlg, h("div.dlg", h("header", h("h2", title)), h("div.body", [message].flat().map((m) => h("p", m))),
      h("footer", cancel ? h("button", { type: "button", onclick: () => done(null) }, "Cancel") : null, choices)));
    dlg.addEventListener("close", () => done(null));
    document.body.append(dlg);
    dlg.showModal();
    (choices.find((b) => b.classList.contains("primary")) || choices[0])?.focus();
  });
}

// --- The data folder ---------------------------------------------------------

// dataFolder is where the collection is kept (GET /api/data-folder), or null where
// it can't be changed.
const dataFolder = () => api("GET", "/api/data-folder").catch(() => null);

// changeDataFolder switches to `path` ("" for the default folder): it looks at what is
// there, asks what to do, then has the app restart.
async function changeDataFolder(df, path) {
  const target = path || df.defaultPath;
  let seen;
  try { seen = await api("POST", "/api/data-folder/inspect", { path: target }); } catch (e) { status(e.message, true); return; }
  if (!seen.writable) { status(`TCG Binder can't write to ${target}. Choose another folder.`, true); return; }
  const cards = seen.games.reduce((n, g) => n + g.cards, 0);
  const mode = seen.games.length
    ? await askDialog("Use this folder?", [`This folder has a collection of ${plural(cards, "unique card", "unique cards")}: ${target}`, "Use it? Your current collection stays where it is."], [{ label: "Use it", value: "use", primary: true }])
    : await askDialog("Move your collection?", [`Move your collection to ${target}, or start fresh there?`, "Moving takes your cards, price history and backups along."], [{ label: "Start fresh", value: "use" }, { label: "Move it here", value: "move", primary: true }]);
  if (!mode) return;
  try {
    const r = await api("POST", "/api/data-folder", { path, mode });
    if (r.restartsItself) status(`Using ${r.path}. Restarting…`);
    else await askDialog("Restart TCG Binder", [`Your collection is now kept in ${r.path}.`, "Stop TCG Binder and start it again to use it. Until then, changes can't be saved."], [{ label: "OK", value: true, primary: true }], false);
  } catch (e) { status(e.message, true); }
}

// chooseDataFolder shows the system's folder picker, then switches to the folder.
async function chooseDataFolder(df) {
  try {
    const r = await api("POST", "/api/data-folder/pick");
    if (r.path) await changeDataFolder(df, r.path);
  } catch (e) { status(e.message, true); }
}

// dataSection is Settings → Data: where the collection is kept, and changing it.
function dataSection(df, fallback) {
  const intro = h("p.muted", "Your collections, price history and backups are kept in this folder. Choose a folder in iCloud Drive, Dropbox or OneDrive to share them between devices. Settings stay on each device.");
  if (!df) return [intro, h("p.settings-path", h("code", fallback || "(not saved)"))];
  const where = h("p.settings-path", h("code", df.path), df.isDefault ? h("span.muted", " (the default)") : null);
  if (df.fixed) return [intro, where, h("p.muted", "Set by --data-dir or TCGBINDER_DATA_DIR when TCG Binder was started.")];
  if (state.info.client === "phone") return [intro, where];
  const useDefault = df.isDefault ? null : h("button", { type: "button", onclick: () => changeDataFolder(df, "") }, "Use the default folder");
  if (df.canPick) return [intro, where, h("div.buttons", h("div.right", h("button", { type: "button", onclick: () => chooseDataFolder(df) }, "Change…"), useDefault))];
  const field = h("input", { type: "text", placeholder: "/full/path/to/a/folder", "aria-label": "Data folder path" });
  return [intro, where, h("div.search", field, h("button", { type: "button", onclick: () => field.value.trim() && changeDataFolder(df, field.value.trim()) }, "Use this folder")),
    h("p.muted", "A folder on the computer running TCG Binder."), useDefault ? h("p", useDefault) : null];
}

// dataNotices are Home's notices about the data folder: the first-run welcome and
// conflict copies another device left.
function dataNotices(df, home) {
  if (!df) return [];
  const out = [];
  if (df.firstRun && !home.cards && !store("welcomed")) {
    const panel = h("div.card-box.notice",
      h("h2", "Welcome to TCG Binder"),
      h("p", `Your collection will be kept in ${df.path}. Keep it there, or choose a folder (for example in iCloud Drive or Dropbox to share it between devices).`),
      h("div.buttons", h("div.right",
        h("button", { type: "button", onclick: () => { store("welcomed", "1"); panel.remove(); } }, "Keep it there"),
        df.canPick ? h("button.primary", { type: "button", onclick: () => chooseDataFolder(df) }, "Choose a folder…")
          : h("a.button", { href: "#/settings" }, "Choose a folder…"))));
    out.push(panel);
  }
  for (const c of df.conflicts.filter((c) => c.game === state.game)) {
    const act = async (what) => {
      if (what === "discard" && !(await confirmDialog(`Delete ${c.name}? Its changes are lost.`, { danger: true, action: "Discard it" }))) return;
      try {
        const r = await api("POST", gameAPI(`/conflicts/${what}`), { name: c.name });
        const unit = { lots: ["lot", "lots"], sales: ["sale", "sales"], lists: ["list", "lists"] }[c.kind] || ["card", "cards"];
        status(what === "merge" ? `Merged ${c.name}: ${plural(r.changed, ...unit)} changed.` : `Discarded ${c.name}.`, false, r.undo);
        await loadFilters();
        route();
      } catch (e) { status(e.message, true); }
    };
    const about = {
      lots: ["your lots", "Merging adds the lots only it has; where both have a lot, the newer one is kept."],
      sales: ["your sales", "Merging adds the sales only it has; none is counted twice."],
      lists: ["your lists", "Merging adds the lists and cards only it has; where both have a card on a list, the larger quantity is kept."],
    }[c.kind] || ["your collection", "Merging adds its cards and copies to yours without counting any twice; a card deleted on one device comes back."];
    out.push(h("div.card-box.notice",
      h("h2", `Another device saved a different version of ${about[0]}`),
      h("p", `${c.name}${c.kind === "collection" && c.cards !== null ? ` (${plural(c.cards, "unique card", "unique cards")})` : ""} was saved by another device at the same time as this one. ${about[1]} Discarding deletes that file.`),
      h("div.buttons", h("div.right",
        h("button", { type: "button", onclick: () => act("discard") }, "Discard it"),
        h("button.primary", { type: "button", onclick: () => act("merge") }, "Merge it")))));
  }
  return out;
}

// --- Charts ------------------------------------------------------------------

const SVG = "http://www.w3.org/2000/svg";
function s(tag, attrs, text) {
  const el = document.createElementNS(SVG, tag);
  for (const [k, v] of Object.entries(attrs || {})) el.setAttribute(k, v);
  if (text !== undefined) el.textContent = text;
  return el;
}
function niceCeil(v) {
  if (v <= 0) return 1;
  const p = Math.pow(10, Math.floor(Math.log10(v)));
  for (const m of [1, 2, 2.5, 5, 10]) if (v <= m * p) return m * p;
  return 10 * p;
}
const day = (d) => new Date(d + "T00:00:00Z").getTime();

// lineChart draws series [{name, color, points:[{date,value}], step}] over [from, to],
// width pixels wide (so text stays readable on phones).
function lineChart(series, from, to, width) {
  const W = Math.round(Math.max(300, Math.min(900, width || 900))), H = W < 600 ? 220 : 280, L = 64, R = 10, T = 10, B = 26;
  const svg = s("svg", { viewBox: `0 0 ${W} ${H}`, class: "chart", role: "img", "aria-label": "Value over time" });
  const pts = series.flatMap((x) => x.points);
  if (!pts.length) {
    svg.append(s("text", { x: W / 2, y: H / 2, "text-anchor": "middle" }, "Add cards to see value over time"));
    return svg;
  }
  let t0 = from ? day(from) : Math.min(...pts.map((p) => day(p.date)));
  let t1 = to ? day(to) : Math.max(...pts.map((p) => day(p.date)));
  if (t1 <= t0) { t0 -= 15 * 864e5; t1 += 15 * 864e5; }
  const yMax = niceCeil(Math.max(...pts.map((p) => p.value)) * 1.05);
  const x = (d) => L + ((day(d) - t0) / (t1 - t0)) * (W - L - R);
  const y = (v) => T + (H - T - B) * (1 - v / yMax);
  for (let i = 0; i <= 4; i++) {
    const v = (yMax * i) / 4;
    svg.append(s("line", { x1: L, x2: W - R, y1: y(v), y2: y(v), class: "grid" }));
    svg.append(s("text", { x: L - 6, y: y(v) + 4, "text-anchor": "end" }, usd(v)));
  }
  const long = t1 - t0 > 400 * 864e5;
  svg.style.height = `${H}px`;
  const fmt = (t) => new Date(t).toLocaleDateString("en-US", long ? { month: "short", year: "numeric", timeZone: "UTC" } : { month: "short", day: "numeric", timeZone: "UTC" });
  [[t0, "start"], [(t0 + t1) / 2, "middle"], [t1, "end"]].forEach(([t, anchor]) => {
    svg.append(s("text", { x: L + ((t - t0) / (t1 - t0)) * (W - L - R), y: H - 6, "text-anchor": anchor }, fmt(t)));
  });
  for (const ser of series) {
    if (!ser.points.length) continue;
    let d = "";
    ser.points.forEach((p, i) => {
      const px = x(p.date), py = y(p.value);
      if (i === 0) d += `M${px},${py}`;
      else d += ser.step ? `H${px}V${py}` : `L${px},${py}`;
    });
    svg.append(s("path", { d, fill: "none", stroke: ser.color, "stroke-width": 2 }));
    if (ser.points.length <= 40) {
      for (const p of ser.points) {
        const dot = s("circle", { cx: x(p.date), cy: y(p.value), r: 3, fill: ser.color });
        dot.append(s("title", {}, `${ser.name} ${p.date}: ${usd(p.value)}`));
        svg.append(dot);
      }
    }
  }
  return svg;
}

function legend(series) {
  return h("div.legend", series.filter((x) => x.points.length).map((x) => h("span", h("i", { style: `background:${x.color}` }), x.name)));
}

function bars(items, max, color) {
  return h("div.bars", items.flatMap((it) => [
    it.onclick ? h("button.label.link", { type: "button", title: it.label, onclick: it.onclick }, it.label) : h("div.label", { title: it.label }, it.label),
    h("div.track", h("div.fill", { style: `width:${Math.max(0, Math.min(1, it.value / (max || 1))) * 100}%;background:${color}` })),
    h("div.num", it.text),
  ]));
}

function tile(cap, val, det, color) {
  return h("div.tile", h("div.cap", cap), h("div.val", { style: color ? `color:${color}` : null }, val), h("div.det", det || ""));
}

function gain(cost, market) {
  const diff = market - cost;
  const pct = cost > 0 ? `${diff >= 0 ? "+" : ""}${((diff / cost) * 100).toFixed(1)}% vs. price paid` : "";
  return { text: (diff >= 0 ? "+" : "") + usd(diff), pct, color: diff >= 0 ? "var(--green)" : "var(--red)" };
}

function cardRows(cards, amount) {
  return h("div.rows", cards.map((c, i) => h("button.row", { type: "button", onclick: () => openCard(c.key) },
    h("span.name", `${i + 1}. ${c.name}`),
    h("span.where", [sn(c), copiesSummary(c), englishPrice(c)].filter(Boolean).join(" · ")),
    h("span.amt", usd(amount(c))))));
}

// --- Home --------------------------------------------------------------------

async function viewHome() {
  const range = state.range;
  const [o, df] = await Promise.all([api("GET", gameAPI(`/home?range=${encodeURIComponent(range)}`)), dataFolder()]);
  const g = gain(o.pricePaid, o.marketValue);
  const series = [
    { name: "Market value", color: "var(--blue)", points: o.chart.marketValue },
    { name: "Price paid", color: "var(--amber)", points: o.chart.pricePaid, step: true },
    { name: "At today's prices (estimate)", color: "var(--grey)", points: o.chart.estimate, step: true },
  ];
  const notes = [];
  if (o.canLoadHistory && !o.historyLoaded) notes.push(`Market value uses daily prices recorded each day the app runs. Load ${o.historyInfo} to fill in the history.`);
  else if (o.chart.historyStart) notes.push(`Market value: daily market prices since ${o.chart.historyStart}.`);
  notes.push("Price paid: running total by acquired date.");
  if (o.chart.estimate.length) notes.push("Before recorded prices begin, the grey line values the cards you owned at today's prices.");

  const loadHistory = o.canLoadHistory && !o.historyLoaded
    ? h("button", { type: "button", onclick: (e) => runJob(e.currentTarget, "/price-history", "Loading price history… this downloads about 50 MB.", (r) => `Loaded price history for ${plural(r.printings, "printing", "printings")}`) }, "Load price history")
    : null;

  const completion = o.bySet.map((x) => ({
    onclick: () => go("collection", new URLSearchParams({ set: x.Code })),
    label: `${x.Code} · ${x.Name}`,
    value: x.Size > 0 ? x.Completion : 0,
    text: x.Size > 0 ? `${x.Collected} / ${x.Size}  (${Math.round(x.Completion * 100)}%)` : `${x.Collected} owned`,
  }));
  const byValue = o.bySet.filter((x) => x.MarketValue > 0).sort((a, b) => b.MarketValue - a.MarketValue).slice(0, 10)
    .map((x) => ({ onclick: () => go("collection", new URLSearchParams({ set: x.Code })), label: `${x.Code} · ${x.Name}`, value: x.MarketValue, text: usd(x.MarketValue) }));

  fill($("#main"),
    h("h1", o.name),
    dataNotices(df, o),
    priceAlerts(o.alerts || []),
    h("div.tiles",
      tile("CARDS", o.cards.toLocaleString(), `${plural(o.uniqueCards, "unique card", "unique cards")} · ${plural(o.sets, "set", "sets")}`),
      tile("PRICE PAID", usd(o.pricePaid), o.pricePaidUnknown ? `${plural(o.pricePaidUnknown, "card", "cards")} without a price paid` : "every card has a price paid"),
      tile("MARKET VALUE", usd(o.marketValue), [`prices as of ${o.pricesAsOf || "never"}${o.unpriced ? ` · ${o.unpriced} unpriced` : ""}`,
        ...(o.sealed ? [h("br"), h("a", { href: "#/sealed" }, `plus ${usd(o.sealed.value)} sealed`)] : [])], "var(--blue)"),
      tile("GAIN / LOSS", g.text, [g.pct, o.hasSales ? `realised: ${signedUsd(o.realisedThisYear)} this year` : ""].filter(Boolean).join(" · "), g.color)),
    h("div.section-head", h("h2", "Value Over Time"),
      h("div.ranges", state.info.ranges.map((r) => h("button" + (r === range ? ".on" : ""), {
        type: "button", onclick: async () => { state.range = r; await saveSettings({ chart_range: r }, "chart range"); route(); } }, r)))),
    h("div.card-box", legend(series), lineChart(series, o.chart.from, o.chart.to, $("#main").clientWidth - 70), h("p.note", notes.join(" ")), loadHistory),
    h("div.two-col",
      h("div", h("h2", "Set Completion"), completion.length ? h("div.card-box", bars(completion, 1, "var(--blue)")) : h("p.muted", "No sets yet.")),
      h("div", h("h2", "Value by Set"), byValue.length ? h("div.card-box", bars(byValue, byValue[0].value, "var(--amber)")) : h("p.muted", "No market prices yet."))),
    h("h2", "Most Valuable Cards"),
    o.topCards.length ? h("div.card-box", cardRows(o.topCards, (c) => c.market_value)) : h("p.muted", "No market prices yet; they refresh automatically each day."),
  );
}

async function runJob(button, path, busy, done) {
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

// --- Collection --------------------------------------------------------------

// ticks is the ticked cards of the current game.
function ticks() {
  if (!state.ticked.has(state.game)) state.ticked.set(state.game, new Map());
  return state.ticked.get(state.game);
}
const tickInfo = (c) => ({ name: c.name, where: sn(c), tags: cardTagList(c) });

// --- Tags --------------------------------------------------------------------

// The core's limits on tags (crates/core/src/collection/tags.rs).
const TAG_MAX_LEN = 40;
const TAGS_MAX = 20;

// tagProblem says why `raw` can't be a tag, or "" if it can (as clean_tag decides).
function tagProblem(raw) {
  const clean = tidyTag(raw);
  if (/[\u0000-\u001f\u007f-\u009f]/.test(raw)) return "A tag can't contain control characters.";
  if (!clean) return "A tag can't be empty.";
  if ([...clean].length > TAG_MAX_LEN) return `A tag can be at most ${TAG_MAX_LEN} characters.`;
  if (/[,;]/.test(clean)) return "A tag can't contain a comma or a semicolon.";
  return "";
}
const tidyTag = (raw) => raw.trim().split(/\s+/).filter(Boolean).join(" ");
const sameTag = (a, b) => a.toLowerCase() === b.toLowerCase();
const sortTags = (tags) => tags.sort((a, b) => a.toLowerCase().localeCompare(b.toLowerCase()));
// cardTagList is a card's tags: the union across its purchases.
const cardTagList = (c) => sortTags([...new Map((c.purchases || []).flatMap((p) => p.tags || []).map((t) => [t.toLowerCase(), t])).values()]);

// tagHue gives a tag a hue from its name, so the same tag always looks the same.
function tagHue(name) {
  let hash = 0;
  for (const ch of name.toLowerCase()) hash = (hash * 31 + ch.codePointAt(0)) >>> 0;
  return hash % 360;
}
const tagStyle = (name) => `--tag-hue:${tagHue(name)}`;

// tagChip is a tag that does something when clicked (a button).
const tagChip = (name, attrs, ...children) => h("button.tag-chip", { type: "button", style: tagStyle(name), ...attrs }, name, ...children);

// tagEditor edits a list of tags: chips with a remove button, then a text box where
// Enter, a comma or a semicolon adds what's typed and Backspace removes the last chip.
// `suggest` lists tags to offer (default: every tag in use). It returns { el, value,
// focus }; value() adds any text still typed and throws if it can't be a tag.
let tagEditors = 0;
function tagEditor(initial, { suggest, label = "Tags", onChange } = {}) {
  let tags = [...(initial || [])];
  const id = `tag-suggest-${++tagEditors}`;
  const known = () => suggest || state.filters.tags || [];
  const input = h("input.tag-input", { type: "text", list: id, placeholder: tags.length ? "" : "Add a tag", "aria-label": label, autocomplete: "off" });
  const err = h("span.tag-error", { role: "alert" });
  const chips = h("span.tag-chips");
  const datalist = h("datalist", { id });
  const render = () => {
    fill(chips, tags.map((t) => h("span.tag-chip", { style: tagStyle(t) }, t,
      h("button.tag-x", { type: "button", "aria-label": `Remove the tag ${t}`, title: "Remove", onclick: () => { remove(t); input.focus(); } }, "×"))));
    fill(datalist, known().filter((k) => !tags.some((t) => sameTag(t, k))).map((k) => h("option", { value: k })));
    input.placeholder = tags.length ? "" : "Add a tag";
    if (onChange) onChange([...tags]);
  };
  const remove = (t) => { tags = tags.filter((x) => !sameTag(x, t)); render(); };
  // add takes what's typed; false (with the reason shown) if any of it can't be a tag.
  const add = () => {
    err.textContent = "";
    const parts = input.value.split(/[,;]/).map(tidyTag).filter(Boolean);
    for (const part of parts) {
      const problem = tagProblem(part);
      if (problem) { err.textContent = problem; return false; }
      if (tags.some((t) => sameTag(t, part))) continue;
      if (tags.length >= TAGS_MAX) { err.textContent = `At most ${TAGS_MAX} tags.`; return false; }
      tags.push(known().find((k) => sameTag(k, part)) || part); // the collection's spelling
    }
    input.value = "";
    sortTags(tags);
    render();
    return true;
  };
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter" || e.key === "," || e.key === ";") {
      if (input.value.trim() || e.key !== "Enter") { e.preventDefault(); add(); }
    } else if (e.key === "Backspace" && !input.value && tags.length) {
      remove(tags[tags.length - 1]);
    }
  });
  // Picking a suggestion from the list adds it straight away.
  input.addEventListener("input", (e) => { if (!e.inputType && known().some((k) => sameTag(k, input.value))) add(); });
  input.addEventListener("blur", () => { if (input.value.trim()) add(); });
  const el = h("div.tag-editor", chips, input, datalist, err);
  el.addEventListener("click", (e) => { if (e.target === el || e.target === chips) input.focus(); });
  render();
  return {
    el,
    value() {
      if (input.value.trim() && !add()) throw new Error(err.textContent);
      return [...tags];
    },
    focus: () => input.focus(),
  };
}

// The collection list's sort keys (as GET /cards takes them) and labels.
const SORTS = [["name", "Name"], ["set", "Set"], ["number", "Number"], ["price", "Price each"], ["value", "Value"],
  ["gain", "Gain / loss"], ["paid", "Price paid"], ["quantity", "Quantity"], ["acquired", "Acquired"], ["released", "Released"], ["rarity", "Rarity"],
  ["tag", "Tag"]];
// Keys that start ascending when chosen; the rest start with the most first.
const ASCENDING_FIRST = ["name", "number", "tag"];
// Collection URL parameters that aren't filters.
const LIST_KEYS = ["key", "sort", "dir", "view"];
// The filters in the More filters panel.
const MORE_KEYS = ["condition", "language", "card_type", "lot", "price_min", "price_max", "acquired_from", "acquired_to", "min_copies", "has_note", "no_price_paid", "unpriced",
  "tags", "tags_mode", "tags_not", "untagged"];
const FLAGS = [["has_note", "Has a note"], ["no_price_paid", "No price paid"], ["unpriced", "No market price"], ["untagged", "Untagged"]];
// The tags in a filter parameter ("binder A,elves deck").
const paramTags = (q, name) => (q.get(name) || "").split(",").map((t) => t.trim()).filter(Boolean);

// smallImage is Scryfall's small image for a card's normal one.
const smallImage = (uri) => uri.replace("/normal/", "/small/");

// thumbnail is a card's picture in a list row (small) or the grid (normal), when images
// are on. The grid keeps a placeholder without one.
function thumbnail(c, grid) {
  if (!state.settings.show_images || !c.image_uri) return grid ? h("span.thumb.none", c.name) : null;
  const src = grid ? c.image_uri : smallImage(c.image_uri);
  return h("img.thumb", { src, alt: "", loading: "lazy", decoding: "async",
    onerror: (e) => { if (e.target.src !== c.image_uri) e.target.src = c.image_uri; } });
}

// moreFilters is the panel of filters beyond the four menus. It stays as the user left
// it; the chips show what is set while it is closed.
function moreFilters(q, nav, menu, gameFilters, lots, tagCounts) {
  const f = state.filters;
  const set = (name, value) => nav((p) => { value ? p.set(name, value) : p.delete(name); p.delete("key"); });
  const field = (name, attrs) => h("input" + (q.get(name) ? ".on" : ""), { value: q.get(name) || "", ...attrs,
    onchange: (e) => set(name, e.target.value.trim()) });
  const active = MORE_KEYS.filter((k) => q.get(k) && k !== "tags_mode").length + gameFilters.filter((x) => q.get("f." + x.Name)).length;
  const panel = h("details.more", { open: state.moreFilters, ontoggle: (e) => { state.moreFilters = e.target.open; } },
    h("summary", active ? `More filters (${active})` : "More filters"),
    h("div.menus",
      // "Mana value: 3" rather than a bare "3" once chosen.
      gameFilters.map((x) => menu("f." + x.Name, x.Name, x.Options, (o) => `${x.Name}: ${o}`)),
      menu("condition", "Condition", f.conditions, (x) => choiceNames.default_condition[x] || x),
      (f.languages || []).length ? menu("language", "Language", f.languages.map((l) => ({ value: l.code, label: l.name }))) : null,
      lots.length ? menu("lot", "Lot", lots.map((l) => ({ value: l.id, label: l.name }))) : null),
    h("div.more-fields",
      h("label", "Type", field("card_type", { type: "text", placeholder: "e.g. creature" })),
      h("label", "At least", field("min_copies", { type: "number", min: 1, step: 1, placeholder: "copies" })),
      h("label", "Price from", field("price_min", { type: "number", min: 0, step: "0.01", placeholder: "$" })),
      h("label", "Price to", field("price_max", { type: "number", min: 0, step: "0.01", placeholder: "$" })),
      h("label", "Acquired from", field("acquired_from", { type: "date" })),
      h("label", "Acquired to", field("acquired_to", { type: "date" }))),
    tagFilter(q, nav, tagCounts),
    h("div.flags", FLAGS.map(([name, label]) => h("label.check",
      h("input", { type: "checkbox", checked: q.get(name) === "1", onchange: (e) => set(name, e.target.checked ? "1" : "") }), label))),
    h("p.note", "Price is one copy's market price. Acquired matches any purchase in the range."));
  return panel;
}

// tagFilter is More filters' Tags group: every tag in use as a chip that cycles
// through chosen, excluded (struck through) and off, with All/Any when two or more
// are chosen.
function tagFilter(q, nav, tagCounts) {
  if (!tagCounts.length) return null;
  const chosen = paramTags(q, "tags"), excluded = paramTags(q, "tags_not");
  const setTags = (p, name, list) => (list.length ? p.set(name, list.join(",")) : p.delete(name));
  const cycle = (t) => nav((p) => {
    const inc = paramTags(p, "tags"), exc = paramTags(p, "tags_not");
    if (inc.some((x) => sameTag(x, t))) { // chosen → excluded
      setTags(p, "tags", inc.filter((x) => !sameTag(x, t)));
      setTags(p, "tags_not", [...exc, t]);
    } else if (exc.some((x) => sameTag(x, t))) { // excluded → off
      setTags(p, "tags_not", exc.filter((x) => !sameTag(x, t)));
    } else {
      setTags(p, "tags", [...inc, t]);
    }
    if (paramTags(p, "tags").length < 2) p.delete("tags_mode");
    p.delete("key");
  });
  const mode = q.get("tags_mode") === "any" ? "any" : "all";
  return h("div.tag-filter",
    h("div.tag-filter-head", h("span", "Tags"),
      chosen.length >= 2 ? h("div.ranges", { role: "group", "aria-label": "Cards with" },
        [["all", "All of these"], ["any", "Any of these"]].map(([v, label]) => h("button" + (v === mode ? ".on" : ""), {
          type: "button", "aria-pressed": String(v === mode),
          onclick: () => nav((p) => { v === "any" ? p.set("tags_mode", "any") : p.delete("tags_mode"); p.delete("key"); }) }, label))) : null),
    h("div.tag-chips", tagCounts.map((t) => {
      const state = chosen.some((x) => sameTag(x, t.name)) ? "on" : excluded.some((x) => sameTag(x, t.name)) ? "not" : "off";
      const said = { on: "chosen; click to exclude it", not: "excluded; click to clear", off: "click to choose" }[state];
      return tagChip(t.name, { class: `tag-chip ${state}`, "aria-pressed": state === "off" ? "false" : "true",
        "aria-label": `${t.name}, ${plural(t.cards, "card", "cards")}: ${said}`, title: said, onclick: () => cycle(t.name) },
        h("span.count", ` ${t.cards}`));
    })),
    h("p.note", "Click a tag to show cards with it, again to show cards without it, a third time to clear it."));
}

// filterChip names a filter in a removable chip.
function filterChip(k, v, lots) {
  const flag = FLAGS.find(([name]) => name === k);
  if (flag) return flag[1];
  switch (k) {
    case "set": return `Set: ${v.toUpperCase()}`;
    case "rarity": return `Rarity: ${title(v)}`;
    case "finish": return `Finish: ${fname(v)}`;
    case "condition": return `Condition: ${choiceNames.default_condition[v] || v}`;
    case "language": return `Language: ${languageName(v)}`;
    case "card_type": return `Type: ${v}`;
    case "lot": return `Lot: ${lots.find((l) => l.id === v)?.name || v}`;
    case "price_min": return `Price ≥ ${usd(Number(v))}`;
    case "price_max": return `Price ≤ ${usd(Number(v))}`;
    case "acquired_from": return `Acquired from ${v}`;
    case "acquired_to": return `Acquired to ${v}`;
    case "min_copies": return `${v}+ copies`;
    default: return k.startsWith("f.") ? `${k.slice(2)}: ${v}` : `${k}: ${v}`;
  }
}

// filterChips shows each filter in use (not the search) as a chip that removes it,
// one for each tag chosen or excluded.
function filterChips(q, nav, lots) {
  const tagChips = [["tags", "Tag"], ["tags_not", "Not"]].flatMap(([k, word]) => paramTags(q, k).map((t) => {
    const label = `${word}: ${t}`;
    return h("button.chip", { type: "button", "aria-label": `Remove the filter ${label}`, onclick: () => nav((p) => {
      const left = paramTags(p, k).filter((x) => !sameTag(x, t));
      left.length ? p.set(k, left.join(",")) : p.delete(k);
      if (paramTags(p, "tags").length < 2) p.delete("tags_mode");
      p.delete("key");
    }) }, label, h("span", { "aria-hidden": "true" }, " ×"));
  }));
  const chips = [...q.entries()].filter(([k, v]) => v && k !== "q" && !LIST_KEYS.includes(k) && !["tags", "tags_not", "tags_mode"].includes(k)).map(([k, v]) => {
    const label = filterChip(k, v, lots);
    return h("button.chip", { type: "button", "aria-label": `Remove the filter ${label}`,
      onclick: () => nav((p) => { p.delete(k); p.delete("key"); }) }, label, h("span", { "aria-hidden": "true" }, " ×"));
  });
  return chips.length || tagChips.length ? h("div.chips", chips, tagChips) : null;
}

// The collection: filters and every matching card (each with a tick box for bulk
// edit), and beside them the selected card's editor or, when a set is chosen, the
// set's page.
async function viewCollection(params) {
  // Selecting a card redraws the list: keep its scroll position if it lists the same cards.
  const listScroll = document.querySelector(".pane-list .list")?.scrollTop || 0;
  const f = state.filters;
  const q = new URLSearchParams(params);
  const nav = (mutate, replace) => { const p = hashParams(); mutate(p); go("collection", p, replace); };
  const menu = (name, label, options, display) => h("select" + (q.get(name) ? ".on" : ""), {
    "aria-label": label, onchange: (e) => nav((p) => { e.target.value ? p.set(name, e.target.value) : p.delete(name); p.delete("key"); }) },
    h("option", { value: "" }, label),
    options.map((o) => h("option", { value: o.value ?? o, selected: q.get(name) === (o.value ?? o) }, display ? display(o) : (o.label ?? o))));

  const search = h("input", { type: "search", placeholder: "Search name, set, or collector number", value: q.get("q") || "", "aria-label": "Search the collection" });
  let timer;
  search.addEventListener("input", () => {
    clearTimeout(timer);
    timer = setTimeout(() => nav((p) => { search.value ? p.set("q", search.value) : p.delete("q"); }, true), 250);
  });
  const setMenu = menu("set", "Set", (f.sets || []).map((x) => ({ value: x.Code, label: x.Label })));
  setMenu.classList.add("set-menu"); // set names are long: the menu gets its own line
  // The game's first filter (Magic's Color) stays in view; the rest go in More filters.
  const [firstFilter, ...otherFilters] = f.filters || [];
  const filters = h("div.filters", search, h("div.menus",
    setMenu,
    firstFilter ? menu("f." + firstFilter.Name, firstFilter.Name, firstFilter.Options) : null,
    menu("rarity", "Rarity", f.rarities, title),
    menu("finish", "Finish", f.finishes, fname)));

  const apiQ = new URLSearchParams(q);
  for (const k of LIST_KEYS) if (k !== "sort" && k !== "dir") apiQ.delete(k);
  const [data, lots, tagTotals] = await Promise.all([api("GET", gameAPI(`/cards?${apiQ}`)), loadLots(),
    (state.filters.tags || []).length ? api("GET", gameAPI("/tags")) : { tags: [] }]);
  const filtering = [...apiQ.keys()].some((k) => !LIST_KEYS.includes(k));
  const grid = q.get("view") === "grid";
  filters.append(...[moreFilters(q, nav, menu, otherFilters, lots, tagTotals.tags), filterChips(q, nav, lots)].filter(Boolean));
  const ticked = ticks();
  if (!filtering) { // every card is listed: drop ticks for cards no longer owned
    const keys = new Set(data.cards.map((c) => c.key));
    for (const k of [...ticked.keys()]) if (!keys.has(k)) ticked.delete(k);
  }

  // Sorting and the list or grid view, kept in the URL with the filters.
  const sortMenu = h("select", { "aria-label": "Sort by", onchange: (e) => nav((p) => {
    p.set("sort", e.target.value);
    p.set("dir", ASCENDING_FIRST.includes(e.target.value) ? "asc" : "desc");
  }, true) }, SORTS.map(([v, label]) => h("option", { value: v, selected: data.sort === v }, `Sort: ${label}`)));
  const desc = data.dir === "desc";
  const dirButton = h("button.dir", { type: "button", "aria-label": desc ? "Descending; switch to ascending" : "Ascending; switch to descending",
    title: desc ? "Descending" : "Ascending",
    onclick: () => nav((p) => { p.set("sort", data.sort); p.set("dir", desc ? "asc" : "desc"); }, true) }, desc ? "↓" : "↑");
  const viewToggle = h("div.ranges", { role: "group", "aria-label": "Show as" },
    [["list", "List"], ["grid", "Grid"]].map(([v, label]) => h("button" + ((v === "grid") === grid ? ".on" : ""), {
      type: "button", "aria-pressed": String((v === "grid") === grid),
      onclick: () => nav((p) => { v === "grid" ? p.set("view", "grid") : p.delete("view"); }, true) }, label)));
  const listbar = h("div.listbar", sortMenu, dirButton, viewToggle);

  const boxes = new Map();
  const row = (c) => {
    const box = h("input", { type: "checkbox", checked: ticked.has(c.key), "aria-label": `Tick ${c.name} for bulk edit`,
      onclick: (e) => e.stopPropagation(),
      onchange: (e) => { e.target.checked ? ticked.set(c.key, tickInfo(c)) : ticked.delete(c.key); updateBar(); } });
    boxes.set(c.key, box);
    const priced = (c.purchases || []).some((p) => p.market > 0);
    // The tick box and the button that opens the card sit side by side.
    return h("div.card" + (c.key === q.get("key") ? ".sel" : "") + (cardTagList(c).length ? ".tagged" : ""),
      box, h("button.entry", { type: "button", onclick: () => openCard(c.key) },
        thumbnail(c, grid),
        h("span.text", h("span.name", c.name), h("span.meta", [sn(c), copiesSummary(c), englishPrice(c), noted(c)].filter(Boolean).join(" · "))),
        h("span.qty", `×${c.copy_count}`),
        // "each" only when every copy is one finish: with two, one price doesn't fit them all.
        h("span.money", priced ? usd(c.market_value) : "—",
          c.copy_count > 1 && priced && new Set(c.purchases.map((p) => p.finish)).size === 1 ? h("small", `${usd(c.each)} each`) : null)),
      rowTags(c));
  };
  // A row's tags: up to three chips (each filters the list by its tag), then "+N".
  const rowTags = (c) => {
    const tags = cardTagList(c);
    if (!tags.length) return null;
    return h("div.row-tags", tags.slice(0, 3).map((t) => tagChip(t, { "aria-label": `Show cards tagged ${t}`, title: `Show cards tagged ${t}`,
      onclick: () => nav((p) => { p.set("tags", t); p.delete("tags_not"); p.delete("tags_mode"); p.delete("key"); }) })),
      tags.length > 3 ? h("span.more", { title: tags.slice(3).join(", ") }, `+${tags.length - 3}`) : null);
  };
  const list = h("div.list" + (grid ? ".grid" : ""), data.cards.length ? data.cards.map(row) : h("p.muted.empty", "No cards match."));

  const counts = `${plural(data.uniqueCards, "unique card", "unique cards")} · ${plural(data.cardCount, "card", "cards")} · ${plural(data.sets, "set", "sets")}`;
  const money = `${usd(data.marketValue)} value · ${usd(data.pricePaid)} paid`;
  const clear = filtering ? h("button.link", { type: "button", onclick: () => {
    const keep = new URLSearchParams([...q.entries()].filter(([k]) => LIST_KEYS.includes(k) && k !== "key"));
    go("collection", keep);
  } }, "Clear filters") : null;
  const exportButton = data.cards.length ? h("button.link", { type: "button", onclick: () => exportListed(q, data.cards.length) }, "Export…") : null;

  // The selection bar: how many are ticked, and the bulk edit actions.
  const count = h("span.count");
  const selectAll = h("button", { type: "button", onclick: () => { data.cards.forEach((c) => ticked.set(c.key, tickInfo(c))); boxes.forEach((b) => { b.checked = true; }); updateBar(); } }, "Select all shown");
  const clearTicks = h("button", { type: "button", onclick: () => { ticked.clear(); boxes.forEach((b) => { b.checked = false; }); updateBar(); } }, "Clear");
  const bulk = h("button.primary", { type: "button", onclick: () => showBulkEdit() }, "Bulk edit…");
  const bar = h("div.selbar", count, selectAll, clearTicks, bulk);
  function updateBar() {
    const hidden = [...ticked.keys()].filter((k) => !boxes.has(k)).length;
    count.textContent = !ticked.size ? "Tick cards to bulk edit them" :
      `${plural(ticked.size, "card", "cards")} ticked${hidden ? ` (${hidden} hidden by filters)` : ""}`;
    bar.classList.toggle("ticked", ticked.size > 0);
    selectAll.disabled = !data.cards.length;
    clearTicks.disabled = bulk.disabled = !ticked.size;
  }
  updateBar();

  const detail = h("div.pane-detail");
  const split = h("div.split", h("div.pane-list", filters,
    h("p.counts.muted", h("span", filtering ? `Matching: ${counts}` : counts, h("span.money-line", ` · ${money}`)), h("span.count-actions", clear, exportButton)),
    listbar, list, bar), detail);
  fill($("#main"), split);
  const listKey = `${apiQ}|${grid}`;
  if (state.listKey === listKey) {
    list.scrollTop = listScroll;
    if (!q.get("key") && state.listWindowY != null) window.scrollTo(0, state.listWindowY);
  }
  if (!q.get("key")) state.listWindowY = null;
  state.listKey = listKey;
  if (q.get("key")) {
    split.classList.add("showing-detail");
    await renderCard(detail, q.get("key"));
  } else if (q.get("set")) {
    await renderSet(detail, q.get("set"));
  } else {
    split.classList.add("no-detail");
    detail.append(h("div.detail", h("h2", { style: "margin-top:0" }, "Your collection"),
      h("p.muted", "Select a card to see and edit it, or choose a set in the Set filter to see its page."),
      h("p.muted", "To change many cards at once, tick them (narrow the list with the search and filters, then Select all shown) and press Bulk edit.")));
  }
}

// showBulkEdit changes or deletes every copy of the ticked cards.
function showBulkEdit() {
  const f = state.filters;
  const ticked = ticks();
  const dlg = $("#dialog");
  if (!ticked.size) {
    fill(dlg, h("div.dlg", h("header", h("h2", "Bulk edit")),
      h("div.body", h("p", "Tick the cards to change in the collection list first. Use the search and filters (set, color, rarity, finish) to narrow the list, then Select all shown.")),
      h("footer", h("button.primary", { type: "button", onclick: () => dlg.close() }, "OK"))));
    dlg.showModal();
    return;
  }
  const chosen = new Map(ticked);
  const counter = h("strong");
  const recount = () => { counter.textContent = `Changing ${plural(chosen.size, "card", "cards")}`; };
  const list = h("div.list", [...chosen].map(([key, c]) => h("label.card",
    h("input", { type: "checkbox", checked: true, onchange: (e) => {
      if (e.target.checked) { chosen.set(key, c); ticked.set(key, c); } else { chosen.delete(key); ticked.delete(key); }
      recount();
    } }),
    h("div.text", h("span.name", c.name), h("span.meta", c.where)))));
  recount();
  const cond = h("select", h("option", { value: "" }, "Unchanged"), f.conditions.map((x) => h("option", { value: x }, (choiceNames.default_condition[x] || x))));
  const fin = h("select", h("option", { value: "" }, "Unchanged"), f.finishes.map((x) => h("option", { value: x }, fname(x))));
  const price = h("input", { type: "number", min: 0, step: "0.01", placeholder: "Unchanged" });
  const date = h("input", { type: "date", "aria-label": "Acquired date" });
  const noteText = h("input", { type: "text", placeholder: "The note", "aria-label": "Note", hidden: true });
  const noteMode = h("select", { "aria-label": "Note", onchange: () => { noteText.hidden = noteMode.value !== "set"; if (!noteText.hidden) noteText.focus(); } },
    h("option", { value: "" }, "Unchanged"), h("option", { value: "set" }, "Set to…"), h("option", { value: "clear" }, "Remove notes"));
  const del = h("input", { type: "checkbox" });
  const addTags = tagEditor([], { label: "Tags to add" });
  // Only the tags the ticked cards have can be removed.
  const onTicked = sortTags([...new Map([...chosen.values()].flatMap((c) => c.tags || []).map((t) => [t.toLowerCase(), t])).values()]);
  const removeTags = tagEditor([], { label: "Tags to remove", suggest: onTicked });
  const clearTags = h("input", { type: "checkbox" });
  const err = h("p.error");
  const apply = async () => {
    err.textContent = "";
    if (!chosen.size) { err.textContent = "Tick at least one card."; return; }
    let add_tags, remove_tags;
    try { add_tags = addTags.value(); remove_tags = removeTags.value(); } catch (e) { err.textContent = e.message; return; }
    const body = { keys: [...chosen.keys()], condition: cond.value, finish: fin.value, acquired_date: date.value, delete: del.checked,
      add_tags, remove_tags, clear_tags: clearTags.checked };
    if (price.value !== "") body.price_paid = parseFloat(price.value);
    if (noteMode.value === "clear") body.note = "";
    if (noteMode.value === "set") {
      if (!noteText.value.trim()) { err.textContent = "Type the note, or choose Unchanged."; noteText.focus(); return; }
      body.note = noteText.value.trim();
    }
    if (!body.delete && !body.condition && !body.finish && !body.acquired_date && body.price_paid === undefined && body.note === undefined
      && !add_tags.length && !remove_tags.length && !body.clear_tags) { err.textContent = "Choose at least one change."; return; }
    const n = plural(chosen.size, "card", "cards");
    if (!(await (body.delete ? ask(`Delete every copy of ${n}? You can undo it right after, or restore a backup from Tools.`) : confirmDialog(`Apply the changes to every copy of ${n}?`, { action: "Apply" })))) return;
    try {
      const r = await api("POST", gameAPI("/bulk"), body);
      status(body.delete ? `Deleted ${plural(r.Deleted, "card", "cards")}.` :
        `Updated ${plural(r.Updated, "card", "cards")}${r.Skipped.length ? `; ${r.Skipped.length} kept their finish (the printing doesn't come in it)` : ""}.`, false, r.undo);
      ticked.clear();
      dlg.close();
      await loadFilters();
      route();
    } catch (e) { err.textContent = e.message; }
  };
  fill(dlg, h("div.dlg",
    h("header", h("h2", "Bulk edit"), h("p.muted", { style: "margin:4px 0 0" }, counter)),
    h("div.body",
      list,
      h("div.form",
        h("label", "Condition"), cond, h("label", "Finish"), fin, h("label", "Price paid (each)"), price, h("label", "Acquired"), date,
        h("label", "Note"), h("div.stack", noteMode, noteText),
        h("label", "Add tags"), addTags.el,
        h("label", "Remove tags"), onTicked.length ? removeTags.el : h("span.muted", "The ticked cards have no tags."),
        h("label", ""), h("label.check", { style: "display:flex;gap:8px;align-items:center;text-align:left" }, clearTags, "Remove all tags"),
        h("label", ""), h("label.check", { style: "display:flex;gap:8px;align-items:center;color:var(--red);text-align:left" }, del, "Delete these cards")),
      h("p.note", "Changes apply to every copy of each card above. Tags are changed in this order: Remove all tags, then Remove tags, then Add tags. Finish changes skip printings that don't come in that finish; purchases that end up the same combine."),
      err),
    h("footer", h("button", { type: "button", onclick: () => { dlg.close(); route(); } }, "Cancel"), h("button.primary", { type: "button", onclick: apply }, "Apply"))));
  dlg.showModal();
}

// renderSet shows a set's completion and value, its most valuable cards (by one copy)
// and, on request, the cards you're missing.
async function renderSet(container, code) {
  const d = await api("GET", gameAPI(`/set?code=${encodeURIComponent(code)}`));
  const g = gain(d.pricePaid, d.marketValue);
  const missingBox = h("div.list.missing", { hidden: true });
  const showMissing = h("button", { type: "button", onclick: async () => {
    showMissing.disabled = true;
    showMissing.textContent = "Loading the set's cards…";
    try {
      const m = await api("GET", gameAPI(`/missing?set=${encodeURIComponent(code)}`));
      missingBox.hidden = false;
      const byNumber = (a, b) => compareNumber(a.collector_number, b.collector_number);
      const byPrice = (a, b) => (anyPrice(b) || 0) - (anyPrice(a) || 0) || byNumber(a, b);
      const sortMenu = h("select", { "aria-label": "Sort the missing cards", onchange: () => show() },
        h("option", { value: "number" }, "By number"), h("option", { value: "price" }, "Most expensive first"));
      const copy = h("button", { type: "button", onclick: () => copyToList(m, code) }, "Copy to a list…");
      const head = m.missing.length ? h("div.missing-head",
        h("p.muted.empty", `${plural(m.missing.length, "card", "cards")} you don't own yet${m.printedSize ? ` (of the ${m.printedSize} printed)` : ""}: ${usd(m.totalCost)} for one of each${m.unpriced ? `, ${m.unpriced} without a price` : ""}. Add one here, or click it to search its printings.`),
        h("div.row-actions", sortMenu, copy)) : h("p.muted.empty", "You have every card in this set.");
      const show = () => fill(missingBox, head, [...m.missing].sort(sortMenu.value === "price" ? byPrice : byNumber).map((c) => {
          const row = h("div.card",
            h("a.rowlink", { href: `#/add?${new URLSearchParams({ text: c.name, set: code })}` },
              h("span.num", c.collector_number), h("span.text", h("span.name", c.name),
                h("span.meta", [title(c.rarity), anyPrice(c) ? usd(anyPrice(c)) : ""].filter(Boolean).join(" · ")))),
            h("div.row-actions", h("button", { type: "button", onclick: () => addToList(c, 1) }, "To list…"),
              h("button", { type: "button", onclick: () => addInDialog(c, () => row.remove()) }, "Add…")));
          return row;
        }));
      show();
      showMissing.remove();
    } catch (e) { status(e.message, true); showMissing.disabled = false; }
  } }, d.size > 0 ? `Show the ${Math.max(d.size - d.collected, 0)} missing cards` : "Show missing cards");
  fill(container, h("div.detail",
    h("h1", d.name || d.code),
    h("p.muted", [d.code, d.released ? `released ${d.released}` : ""].filter(Boolean).join(" · ")),
    h("div.tiles", { style: "grid-template-columns:repeat(auto-fit,minmax(150px,1fr))" },
      tile("COMPLETION", d.size > 0 ? `${Math.round(d.completion * 100)}%` : "—", d.size > 0 ? `${d.collected} of ${d.size} cards` : "set size not loaded yet", "var(--blue)"),
      tile("CARDS", d.cards.toLocaleString(), plural(d.uniqueCards, "unique card", "unique cards")),
      tile("MARKET VALUE", usd(d.marketValue), `price paid ${usd(d.pricePaid)}`, "var(--blue)"),
      tile("GAIN / LOSS", g.text, g.pct, g.color)),
    h("h2", "Most Valuable in This Set"),
    h("p.note", "Ranked by the value of one copy."),
    d.top.length ? cardRows(d.top, (c) => c.each) : h("p.muted", "No market prices yet."),
    h("h2", "Missing Cards"),
    showMissing, missingBox));
}

// signedUsd shows a gain with its sign: "+$1.05", "-$0.60".
const signedUsd = (v) => (v > 0 ? "+" : "") + usd(v);

// showSell sells copies from one of a card's saved purchases `l`.
function showSell(c, key, l) {
  const dlg = $("#dialog");
  const qty = h("input", { type: "number", min: 1, max: l.quantity, step: 1, value: 1, "aria-label": "Copies sold" });
  const each = h("input", { type: "number", min: 0, step: "0.01", value: l.market ? l.market.toFixed(2) : "", placeholder: "0.00", "aria-label": "Sold for, each" });
  const date = h("input", { type: "date", value: today(), "aria-label": "Date sold" });
  const fees = h("input", { type: "number", min: 0, step: "0.01", placeholder: "0.00", "aria-label": "Fees, in all" });
  const note = h("input", { type: "text", placeholder: "Where or to whom", "aria-label": "Note on the sale" });
  const outcome = h("p.note");
  const err = h("p.error");
  const recount = () => {
    const q = parseInt(qty.value, 10) || 0;
    const proceeds = q * (parseFloat(each.value) || 0) - (parseFloat(fees.value) || 0);
    outcome.textContent = l.price_paid
      ? `Paid ${usd(l.price_paid)} each. After fees: ${usd(proceeds)}, a gain of ${signedUsd(proceeds - q * l.price_paid)}.`
      : `After fees: ${usd(proceeds)}. These copies have no price paid, so the sale's gain is unknown and left out of the totals.`;
  };
  [qty, each, fees].forEach((x) => x.addEventListener("input", recount));
  recount();
  const sell = async () => {
    err.textContent = "";
    const q = parseInt(qty.value, 10);
    if (!Number.isInteger(q) || q < 1 || q > l.quantity) { err.textContent = `Sell 1 to ${l.quantity} copies.`; qty.focus(); return; }
    if (each.value === "") { err.textContent = "Type what each copy sold for."; each.focus(); return; }
    try {
      const r = await api("POST", gameAPI(`/card/sell?key=${encodeURIComponent(key)}`), {
        finish: l.finish, condition: l.condition, price_paid: l.price_paid || 0, acquired_date: l.acquired_date || "", note: l.note || "", tags: l.tags || [],
        quantity: q, sale_each: parseFloat(each.value), date: date.value, fees: parseFloat(fees.value) || 0, sale_note: note.value.trim(),
      });
      dlg.close();
      status(`Sold ${plural(q, "copy", "copies")} of ${c.name}${r.gain !== null ? `: ${signedUsd(r.gain)}` : ""}. See Tools → Earnings.`);
      await loadFilters();
      if (q >= c.copy_count) closeCard(); else openCard(key);
    } catch (e) { err.textContent = e.message; }
  };
  fill(dlg, h("div.dlg",
    h("header", h("h2", `Sell ${c.name}`), h("p.muted", { style: "margin:4px 0 0" },
      [sn(c), fname(l.finish), l.condition, plural(l.quantity, "copy", "copies"), l.acquired_date ? `acquired ${l.acquired_date}` : ""].filter(Boolean).join(" · "))),
    h("div.body",
      h("div.form",
        h("label", "Copies"), qty, h("label", "Sold for (each)"), each, h("label", "Date sold"), date,
        h("label", "Fees (in all)"), fees, h("label", "Note"), note),
      outcome,
      h("p.note", "The copies leave your collection and the sale is kept under Tools → Earnings, where it can be undone. Unsaved changes on the card page are discarded."),
      err),
    h("footer", h("button", { type: "button", onclick: () => dlg.close() }, "Cancel"), h("button.primary", { type: "button", onclick: sell }, "Sell"))));
  dlg.showModal();
  each.focus();
}

// viewEarnings lists sold cards with totals, for one year or all of them.
async function viewEarnings(qs) {
  const q = new URLSearchParams(qs);
  const year = q.get("year") || "";
  const d = await api("GET", gameAPI(`/sales?year=${encodeURIComponent(year)}`));
  const t = d.totals;
  const years = h("select", { "aria-label": "Year", onchange: (e) => go("earnings", new URLSearchParams(e.target.value ? { year: e.target.value } : {})) },
    h("option", { value: "" }, "All years"),
    [...new Set([...d.years, year].filter(Boolean))].map((y) => h("option", { value: y, selected: y === year }, y)));
  const undo = async (s) => {
    const what = `${plural(s.quantity, "copy", "copies")} of ${s.card.name}`;
    const choice = await askDialog("Undo this sale?", [
      `${what}, sold ${s.date} for ${usd(s.proceeds)}.`,
      "Putting the copies back returns them to your collection as the purchase they were, and deletes the sale. Deleting the record only leaves them sold.",
    ], [{ label: "Delete the record only", value: "delete", danger: true }, { label: "Put the copies back", value: "restore", primary: true }]);
    if (!choice) return;
    try {
      await api("DELETE", gameAPI(`/sales?id=${encodeURIComponent(s.id)}${choice === "restore" ? "&restore=1" : ""}`));
      status(choice === "restore" ? `Put ${what} back in your collection.` : `Deleted the sale of ${what}.`);
      await loadFilters();
      route();
    } catch (e) { status(e.message, true); }
  };
  const gainText = (v) => (v === null ? "gain unknown" : signedUsd(v));
  const gainColor = (v) => (v === null ? null : v >= 0 ? "var(--green)" : "var(--red)");
  const sale = (s) => h("div.sale",
    h("div.sale-main",
      h("span.name", s.card.name),
      h("span.meta", [s.date, sn(s.card), `${fname(s.finish)} ${s.condition}`, `×${s.quantity} at ${usd(s.sale_each)}`,
        s.fees ? `fees ${usd(s.fees)}` : "", s.cost_each ? `paid ${usd(s.cost_each)} each` : "no price paid", s.note].filter(Boolean).join(" · "))),
    h("span.sale-amt", h("b", usd(s.proceeds)), h("small", { style: gainColor(s.gain) ? `color:${gainColor(s.gain)}` : null }, gainText(s.gain)),
      h("button.link", { type: "button", onclick: () => undo(s) }, "Undo…")));
  const months = d.months.length ? h("div.card-box.wide", h("table.months",
    h("thead", h("tr", h("th", "Month"), h("th.n", "Copies"), h("th.n", "Sold for"), h("th.n", "Fees"), h("th.n", "Gain"))),
    h("tbody", [...d.months].reverse().map((m) => h("tr", h("td", m.month), h("td.n", m.copies), h("td.n", usd(m.proceeds)), h("td.n", usd(m.fees)),
      // A month whose sales all lack a price paid has no gain to show.
      m.unknownCostCopies === m.copies ? h("td.n.muted", "unknown") : h("td.n", { style: `color:${gainColor(m.gain)}` }, signedUsd(m.gain))))))) : null;
  fill($("#main"),
    h("div.section-head", h("h1", "Earnings"), years),
    h("div.tiles",
      tile("SOLD FOR", usd(t.proceeds), `${plural(t.copies, "copy", "copies")} in ${plural(t.sales, "sale", "sales")}`),
      tile("FEES", usd(t.fees), ""),
      tile("COST", usd(t.cost), t.unknownCostCopies ? `${plural(t.unknownCostCopies, "copy", "copies")} without a price paid` : "what the sold copies cost"),
      tile("GAIN", t.unknownCostCopies === t.copies && t.copies ? "—" : signedUsd(t.gain), t.unknownCostCopies ? "sales without a price paid are left out" : "sold for, less fees and cost", gainColor(t.gain))),
    months ? h("h2", "By Month") : null, months,
    h("h2", "Sales"),
    d.sales.length ? h("div.card-box", d.sales.map(sale))
      : h("p.muted", year ? `No sales in ${year}.` : "No sales yet. To sell copies, open a card and choose Sell… on the purchase they came from."));
}

// addToList puts catalog card `c` on a list (or a new one), in a dialog of its own so it
// can open over another.
async function addToList(c, quantity) {
  const lists = await api("GET", gameAPI("/lists")).catch(() => []);
  const dlg = h("dialog.ask");
  const NEW = "\u0000new";
  const last = store("lastList");
  const which = h("select", { "aria-label": "List", onchange: () => { name.hidden = which.value !== NEW; if (!name.hidden) name.focus(); } },
    lists.map((l) => h("option", { value: l.id, selected: l.id === last }, l.name)), h("option", { value: NEW, selected: !lists.length }, "New list…"));
  const name = h("input", { type: "text", placeholder: "Name, e.g. Mono-red burn", "aria-label": "New list's name", hidden: lists.length > 0 });
  const qty = h("input", { type: "number", min: 1, step: 1, value: quantity || 1, "aria-label": "Copies wanted" });
  const any = h("input", { type: "checkbox", checked: true });
  const err = h("p.error");
  const close = () => { dlg.close(); dlg.remove(); };
  const save = async () => {
    err.textContent = "";
    const q = parseInt(qty.value, 10);
    if (!Number.isInteger(q) || q < 1) { err.textContent = "Want at least 1 copy."; qty.focus(); return; }
    try {
      let id = which.value;
      if (id === NEW) {
        if (!name.value.trim()) { err.textContent = "Name the new list."; name.focus(); return; }
        id = (await api("POST", gameAPI("/lists"), { name: name.value.trim() })).id;
      }
      const t = await api("POST", gameAPI(`/lists/entries?id=${encodeURIComponent(id)}`), { card: c, quantity: q, any_printing: any.checked });
      store("lastList", id);
      close();
      status(`Added ${plural(q, "copy", "copies")} of ${c.name} to ${t.name}.`);
    } catch (e) { err.textContent = e.message; }
  };
  fill(dlg, h("div.dlg", h("header", h("h2", "Add to a list"), h("p.muted", { style: "margin:4px 0 0" }, `${c.name} · ${sn(c)}`)),
    h("div.body", h("div.form", h("label", "List"), h("div.stack", which, name), h("label", "Copies wanted"), qty,
      h("label", ""), h("label.check", { style: "display:flex;gap:8px;align-items:center;text-align:left" }, any, "Any printing counts as owned")),
      h("p.note", "Untick to count only this printing as owned."), err),
    h("footer", h("button", { type: "button", onclick: close }, "Cancel"), h("button.primary", { type: "button", onclick: save }, "Add to list"))));
  dlg.addEventListener("close", () => dlg.remove());
  document.body.append(dlg);
  dlg.showModal();
  (lists.length ? qty : name).focus();
}

// viewLists shows every list, or one list (`id`).
async function viewLists(qs) {
  const id = new URLSearchParams(qs).get("id");
  if (id) return viewList(id);
  const lists = await api("GET", gameAPI("/lists"));
  const create = async () => {
    const name = await askName("New list", "");
    if (!name) return;
    try {
      const l = await api("POST", gameAPI("/lists"), { name });
      go("lists", new URLSearchParams({ id: l.id }));
    } catch (e) { status(e.message, true); }
  };
  fill($("#main"),
    h("div.section-head", h("h1", "Lists"), h("button.primary", { type: "button", onclick: create }, "New list")),
    h("p.muted", "Decks and want lists. Each shows how many of its cards you own and what the rest would cost. Add cards from the Add page or a set's Missing Cards, or paste a decklist."),
    lists.length ? h("div.card-box", h("div.rows", lists.map((l) => h("a.row", { href: `#/lists?id=${encodeURIComponent(l.id)}` },
      h("span.name", l.name),
      h("span.where", `${l.owned} of ${plural(l.cards, "card", "cards")} owned${l.missing ? ` · ${usd(l.missingCost)} to complete` : " · complete"}`),
      h("span.amt", usd(l.marketValue))))))
      : h("p.muted", "No lists yet."));
}

// askName asks for a list's name in a dialog; null if cancelled.
function askName(title, value) {
  return new Promise((resolve) => {
    const dlg = h("dialog.ask");
    const input = h("input", { type: "text", value, "aria-label": "Name", placeholder: "e.g. Mono-red burn" });
    let done = false;
    const finish = (v) => { if (done) return; done = true; dlg.close(); dlg.remove(); resolve(v); };
    const ok = () => { if (input.value.trim()) finish(input.value.trim()); else input.focus(); };
    input.addEventListener("keydown", (e) => { if (e.key === "Enter") ok(); });
    fill(dlg, h("div.dlg", h("header", h("h2", title)), h("div.body", input),
      h("footer", h("button", { type: "button", onclick: () => finish(null) }, "Cancel"), h("button.primary", { type: "button", onclick: ok }, "Save"))));
    dlg.addEventListener("close", () => finish(null));
    document.body.append(dlg);
    dlg.showModal();
    input.focus();
  });
}

// viewList is one list: totals, entries (owned of wanted, price), and its actions.
async function viewList(id) {
  const d = await api("GET", gameAPI(`/lists?id=${encodeURIComponent(id)}`));
  const l = d.list;
  const entries = d.entries;
  const put = async (body, done) => {
    try { await api("PUT", gameAPI(`/lists?id=${encodeURIComponent(id)}`), body); if (done) status(done); route(); } catch (e) { status(e.message, true); }
  };
  const forms = () => entries.map((e) => ({ card: e.card, quantity: e.quantity, any_printing: e.any_printing, finish: e.finish }));
  const saveEntries = (change, done) => { const next = forms(); change(next); put({ entries: next.filter(Boolean) }, done); };
  const rename = async () => { const name = await askName("Rename the list", l.name); if (name) put({ name }); };
  const editNotes = async () => {
    const box = h("textarea", { rows: 4, "aria-label": "Notes" }, l.notes || "");
    const v = await askDialog("Notes", [box], [{ label: "Save", value: "save", primary: true }]);
    if (v) put({ notes: box.value });
  };
  const del = async () => {
    if (!(await confirmDialog(`Delete the list ${l.name}? The cards in your collection stay.`, { danger: true, action: "Delete" }))) return;
    try { await api("DELETE", gameAPI(`/lists?id=${encodeURIComponent(id)}`)); status(`Deleted ${l.name}.`); go("lists"); } catch (e) { status(e.message, true); }
  };
  const paste = async () => {
    const box = h("textarea", { rows: 10, "aria-label": "Card list", placeholder: "4 Lightning Bolt (M11) 149\n2 Opt\n1 Shock *F*" });
    const v = await askDialog("Paste a card list", [
      "One card per line, as Moxfield, Archidekt or MTG Arena export them. A line with a set (and number) wants that printing; a line with just a name counts any printing as owned.",
      box,
    ], [{ label: "Add to list", value: "add", primary: true }]);
    if (!v || !box.value.trim()) return;
    try {
      const r = await whileRunning(api("POST", gameAPI(`/lists/import?id=${encodeURIComponent(id)}`), new Blob([box.value])));
      status(`Added ${plural(r.added, "card", "cards")} to ${l.name}.${r.unmatched.length ? ` Not found: ${r.unmatched.join("; ")}` : ""}`, r.unmatched.length > 0);
      route();
    } catch (e) { status(e.message, true); }
  };
  const exportList = () => { location.href = gameAPI(`/lists/export?id=${encodeURIComponent(id)}`); };

  const row = (e, i) => {
    const c = e.card;
    const have = Math.min(e.owned, e.quantity);
    const qty = h("input", { type: "number", min: 1, step: 1, value: e.quantity, "aria-label": `Copies of ${c.name} wanted`,
      onchange: () => { const q = parseInt(qty.value, 10); if (q >= 1) saveEntries((n) => { n[i].quantity = q; }); } });
    const any = h("input", { type: "checkbox", checked: e.any_printing, "aria-label": "Any printing counts",
      onchange: () => saveEntries((n) => { n[i].any_printing = any.checked; }) });
    return h("div.list-entry" + (have >= e.quantity ? ".have" : ""),
      thumbnail(c, false),
      h("div.text",
        h("span.name", c.name),
        h("span.meta", [e.any_printing ? "any printing" : sn(c), e.finish ? fname(e.finish) : "", e.price !== null ? `${usd(e.price)} each` : "no price"].filter(Boolean).join(" · ")),
        h("label.check.small", any, "Any printing")),
      h("div.entry-controls",
        h("span.owned-count", { title: "Owned of wanted" }, `${have} / ${e.quantity}`),
        qty,
        have < e.quantity ? h("button", { type: "button", onclick: () => addInDialog(c, () => route()) }, "Add…") : h("span.done", "✓"),
        h("button", { type: "button", "aria-label": `Remove ${c.name} from the list`, title: "Remove from the list",
          onclick: () => saveEntries((n) => { n[i] = null; }, `Removed ${c.name} from ${l.name}.`) }, "✕")));
  };

  fill($("#main"),
    h("p", h("a", { href: "#/lists" }, "← Lists")),
    h("div.section-head", h("h1", l.name), h("div.buttons",
      h("button", { type: "button", onclick: rename }, "Rename"), h("button", { type: "button", onclick: editNotes }, "Notes"),
      h("button", { type: "button", onclick: paste }, "Paste a list…"), h("button", { type: "button", onclick: exportList }, "Export"),
      h("button.danger", { type: "button", onclick: del }, "Delete"))),
    l.notes ? h("p.list-notes", l.notes) : null,
    h("div.tiles",
      tile("CARDS", l.cards.toLocaleString(), plural(l.entries, "entry", "entries")),
      tile("OWNED", `${l.owned}`, l.cards ? `${Math.round((l.owned / l.cards) * 100)}% of the list` : ""),
      tile("TO COMPLETE", usd(l.missingCost), l.missing ? `${plural(l.missing, "card", "cards")} missing, at market price` : "you own every card", "var(--blue)"),
      tile("LIST VALUE", usd(l.marketValue), "every card, at market price")),
    entries.length ? h("div.card-box", entries.map(row))
      : h("p.muted", "No cards yet. Add some from the Add page (Add to list…), from a set's Missing Cards, or with Paste a list…"));
}

// ruleText describes a watch's rule: "a 20% move", "at or above $5.00".
function ruleText(r) {
  if (r.kind === "percent") return `a ${r.value}% move either way`;
  return `${r.kind === "above" ? "at or above" : "at or below"} ${usd(r.value)}`;
}

// The watch's change since its baseline: "+$0.05 (+25%)".
const watchChange = (w) => (w.change === null ? "" : `${signedUsd(w.change)} (${w.changePercent > 0 ? "+" : ""}${w.changePercent}%)`);

const ALERT_NOTE = "Prices are checked when the app refreshes them (each day, or as Settings says), so alerts appear while the app is open or the next time you open it. There is no email or text message.";

// showWatch starts watching catalog card `c` (in `finish`, if given), or changes its
// alert when `w` (a watch) is given.
function showWatch(c, finish, w) {
  const dlg = h("dialog.ask");
  const priced = (c.finishes && c.finishes.length ? c.finishes : Object.keys(c.market || {}));
  const fin = h("select", { "aria-label": "Finish", disabled: !!w },
    (w ? [w.finish] : priced).map((f) => h("option", { value: f, selected: f === (w ? w.finish : finish) },
      `${fname(f)}${c.market && c.market[f] ? ` · ${usd(c.market[f])}` : " · no price"}`)));
  const kind = h("select", { "aria-label": "Alert when" },
    [["percent", "It moves by (%)"], ["above", "It reaches or passes ($)"], ["below", "It falls to or under ($)"]]
      .map(([v, label]) => h("option", { value: v, selected: v === (w ? w.rule.kind : "percent") }, label)));
  const value = h("input", { type: "number", min: 0, step: "0.01", value: w ? w.rule.value : 20, "aria-label": "Amount" });
  const err = h("p.error");
  const close = () => { dlg.close(); dlg.remove(); };
  const save = async () => {
    err.textContent = "";
    const v = parseFloat(value.value);
    if (!(v > 0)) { err.textContent = "Type a number above 0."; value.focus(); return; }
    const rule = { kind: kind.value, value: v };
    try {
      if (w) await api("PUT", gameAPI(`/watchlist?key=${encodeURIComponent(w.key)}`), { rule });
      else await api("POST", gameAPI("/watchlist"), { card: c, finish: fin.value, rule });
      close();
      status(w ? `Changed the alert for ${c.name}.` : `Watching ${c.name} (${fname(fin.value)}). See Tools → Watchlist.`);
      if (w) route();
    } catch (e) { err.textContent = e.message; }
  };
  fill(dlg, h("div.dlg", h("header", h("h2", w ? "Change the alert" : "Watch the price"), h("p.muted", { style: "margin:4px 0 0" }, `${c.name} · ${sn(c)}`)),
    h("div.body", h("div.form", h("label", "Finish"), fin, h("label", "Alert when"), kind, h("label", "Amount"), value),
      h("p.note", ALERT_NOTE), err),
    h("footer", h("button", { type: "button", onclick: close }, "Cancel"), h("button.primary", { type: "button", onclick: save }, w ? "Save" : "Watch"))));
  dlg.addEventListener("close", () => dlg.remove());
  document.body.append(dlg);
  dlg.showModal();
  value.focus();
}

// watchAction dismisses or resets a watch's alert, or removes the watch.
async function watchAction(w, action) {
  try {
    if (action === "remove") await api("DELETE", gameAPI(`/watchlist?key=${encodeURIComponent(w.key)}`));
    else await api("PUT", gameAPI(`/watchlist?key=${encodeURIComponent(w.key)}`), { action });
    status({ dismiss: `Dismissed the alert for ${w.card.name}.`, reset: `${w.card.name} is now measured from today's price.`, remove: `Stopped watching ${w.card.name}.` }[action]);
    route();
  } catch (e) { status(e.message, true); }
}

// priceAlerts is Home's box of triggered watches.
function priceAlerts(alerts) {
  if (!alerts.length) return null;
  return h("div.card-box.notice",
    h("h2", "Price alerts"),
    alerts.map((w) => h("div.alert-row",
      h("div.text", h("span.name", `${w.card.name} · ${fname(w.finish)}`),
        h("span.meta", `${usd(w.baseline)} on ${w.baseline_date} → ${usd(w.triggered.price)} on ${w.triggered.date}${w.change !== null ? ` · ${watchChange(w)}` : ""} · alert: ${ruleText(w.rule)}`)),
      h("div.row-actions",
        h("button", { type: "button", title: "Hide this alert; it comes back if the price returns and crosses again", onclick: () => watchAction(w, "dismiss") }, "Dismiss"),
        h("button", { type: "button", title: "Measure from today's price from now on", onclick: () => watchAction(w, "reset") }, "Reset")))),
    h("p.note", h("a", { href: "#/watchlist" }, "Watchlist"), ` · ${ALERT_NOTE}`));
}

// viewWatchlist lists the watched cards: price, baseline, change and alert.
async function viewWatchlist() {
  const list = await api("GET", gameAPI("/watchlist"));
  const row = (w) => h("div.list-entry" + (w.triggered ? ".alerted" : ""),
    thumbnail(w.card, false),
    h("div.text",
      h("span.name", `${w.card.name}`),
      h("span.meta", [sn(w.card), fname(w.finish), `since ${w.baseline_date}: ${w.baseline ? usd(w.baseline) : "no price"} → ${w.price !== null ? usd(w.price) : "no price"}`].join(" · ")),
      h("span.meta", `Alert: ${ruleText(w.rule)}${w.triggered ? ` · triggered ${w.triggered.date} at ${usd(w.triggered.price)}` : ""}`)),
    h("div.entry-controls",
      h("span.change" + (w.change > 0 ? ".up" : w.change < 0 ? ".down" : ""), watchChange(w) || "—"),
      h("button", { type: "button", onclick: () => showWatch(w.card, w.finish, w) }, "Change…"),
      w.triggered ? h("button", { type: "button", onclick: () => watchAction(w, "dismiss") }, "Dismiss") : null,
      h("button", { type: "button", title: "Measure from today's price", onclick: () => watchAction(w, "reset") }, "Reset"),
      h("button", { type: "button", "aria-label": `Stop watching ${w.card.name}`, title: "Stop watching", onclick: () => watchAction(w, "remove") }, "✕")));
  fill($("#main"),
    h("h1", "Watchlist"),
    h("p.muted", ALERT_NOTE),
    list.length ? h("div.card-box", list.map(row))
      : h("p.muted", "Nothing watched yet. Choose Watch… on the Add page's card form, or Watch price… on a card you own."));
}

// productKind names a sealed product's category and subtype: "Booster box · Draft".
const productKind = (p) => [p.category, p.subtype !== "default" ? p.subtype : ""].filter(Boolean).map((x) => title(x.replaceAll("_", " "))).join(" · ");

// showSealed adds a sealed item, or edits `item`.
function showSealed(item) {
  const it = item || {};
  const dlg = h("dialog.ask");
  let chosen = it.product_uuid ? { uuid: it.product_uuid, category: it.category, subtype: it.subtype, tcgplayer_product_id: it.tcgplayer_product_id, purchase_url: it.purchase_url } : null;
  const set = h("input", { type: "text", value: it.set || "", placeholder: "Set code, e.g. M19", "aria-label": "Set code", autocapitalize: "characters" });
  const find = h("button", { type: "button" }, "Find products");
  const pick = h("select", { "aria-label": "Product", hidden: true });
  const name = h("input", { type: "text", value: it.name || "", placeholder: "Product name", "aria-label": "Product name" });
  const qty = h("input", { type: "number", min: 1, step: 1, value: it.quantity || 1, "aria-label": "Quantity" });
  const paid = h("input", { type: "number", min: 0, step: "0.01", value: it.price_paid || "", placeholder: "Each", "aria-label": "Price paid each" });
  const date = h("input", { type: "date", value: it.acquired_date || (state.settings.default_date === "today" ? today() : ""), "aria-label": "Acquired" });
  const value = h("input", { type: "number", min: 0, step: "0.01", value: it.value || "", placeholder: "Each, as you look it up", "aria-label": "Value each" });
  const note = h("input", { type: "text", value: it.note || "", placeholder: "Where it's kept, condition…", "aria-label": "Note" });
  const found = h("p.note");
  const err = h("p.error");
  let products = [];
  const choose = () => {
    const p = products[parseInt(pick.value, 10)];
    chosen = p || null;
    if (p) name.value = p.name;
    else { name.value = ""; name.focus(); }
  };
  pick.addEventListener("change", choose);
  find.addEventListener("click", async () => {
    err.textContent = "";
    if (!set.value.trim()) { err.textContent = "Type the set's code first."; set.focus(); return; }
    find.disabled = true;
    found.textContent = "Looking up the set's products…";
    try {
      products = (await api("GET", gameAPI(`/sealed/products?set=${encodeURIComponent(set.value.trim())}`))).products;
      fill(pick, h("option", { value: "" }, products.length ? "Choose a product, or type a name below" : "Type a name below"),
        products.map((p, i) => h("option", { value: i }, `${p.name} (${productKind(p)})`)));
      pick.hidden = !products.length;
      found.textContent = products.length ? `${plural(products.length, "product", "products")} in ${set.value.trim().toUpperCase()}.` : `No products listed for ${set.value.trim().toUpperCase()}; type the name.`;
      if (products.length) pick.focus();
    } catch (e) { found.textContent = ""; err.textContent = e.message; }
    find.disabled = false;
  });
  name.addEventListener("input", () => { if (chosen && name.value !== (products.find((p) => p.uuid === chosen.uuid) || {}).name && !item) chosen = null; });
  const close = () => { dlg.close(); dlg.remove(); };
  const save = async () => {
    err.textContent = "";
    const body = {
      name: name.value.trim(), set: set.value.trim(), quantity: parseInt(qty.value, 10), price_paid: parseFloat(paid.value) || 0,
      acquired_date: date.value, value: parseFloat(value.value) || 0, value_date: it.value_date || "", // the server dates a changed value today
      note: note.value.trim(), product_uuid: chosen ? chosen.uuid : "", category: chosen ? chosen.category : "", subtype: chosen ? chosen.subtype : "",
      tcgplayer_product_id: chosen ? chosen.tcgplayer_product_id || 0 : 0, purchase_url: chosen ? chosen.purchase_url || "" : "",
    };
    if (!body.name) { err.textContent = "Choose a product or type its name."; name.focus(); return; }
    if (!(body.quantity >= 1)) { err.textContent = "The quantity must be at least 1."; qty.focus(); return; }
    try {
      if (item) await api("PUT", gameAPI(`/sealed?id=${encodeURIComponent(item.id)}`), body);
      else await api("POST", gameAPI("/sealed"), body);
      close();
      status(item ? `Saved ${body.name}.` : `Added ${plural(body.quantity, "item", "items")}: ${body.name}.`);
      route();
    } catch (e) { err.textContent = e.message; }
  };
  fill(dlg, h("div.dlg", h("header", h("h2", item ? "Edit sealed product" : "Add sealed product")),
    h("div.body",
      h("div.form",
        h("label", "Set"), h("div.inline", set, find),
        h("label", "Product"), h("div.stack", pick, name),
        h("label", "Quantity"), qty, h("label", "Price paid (each)"), paid, h("label", "Acquired"), date,
        h("label", "Value (each)"), value, h("label", "Note"), note),
      found,
      h("p.note", "There's no free price source for sealed product, so the value is yours to enter: look it up (the product's link on the Sealed page helps) and update it now and then. Values older than 90 days are marked."),
      err),
    h("footer", h("button", { type: "button", onclick: close }, "Cancel"), h("button.primary", { type: "button", onclick: save }, item ? "Save" : "Add"))));
  dlg.addEventListener("close", () => dlg.remove());
  document.body.append(dlg);
  dlg.showModal();
  (item ? value : set).focus();
}

// viewSealed lists sealed product with what was paid and what it's worth.
async function viewSealed() {
  const d = await api("GET", gameAPI("/sealed"));
  const t = d.totals;
  // Gain over the items with both a price paid and a value: the others would skew it.
  const both = d.items.filter((i) => i.gain !== null);
  const g = both.length ? gain(both.reduce((n, i) => n + i.price_paid * i.quantity, 0), both.reduce((n, i) => n + i.value * i.quantity, 0)) : null;
  const left = d.items.length - both.length;
  const remove = async (i) => {
    if (!(await ask(`Remove ${i.name} from your sealed product?`, "Remove"))) return;
    try { await api("DELETE", gameAPI(`/sealed?id=${encodeURIComponent(i.id)}`)); status(`Removed ${i.name}.`); route(); } catch (e) { status(e.message, true); }
  };
  const row = (i) => h("div.list-entry" + (i.valueStale ? ".stale" : ""),
    h("div.text",
      h("span.name", i.name),
      h("span.meta", [i.set, productKind(i), `×${i.quantity}`, i.price_paid ? `paid ${usd(i.price_paid)} each` : "no price paid", i.acquired_date, i.note].filter(Boolean).join(" · ")),
      h("span.meta", i.value ? `worth ${usd(i.value)} each, as of ${i.value_date || "—"}${i.valueStale ? " · over 90 days old: check it" : ""}` : "no value yet: look it up and enter it",
        i.purchase_url ? [" · ", h("a", { href: i.purchase_url, target: "_blank", rel: "noopener noreferrer" }, "Look up a price ↗")] : null)),
    h("div.entry-controls",
      h("span.change" + (i.gain > 0 ? ".up" : i.gain < 0 ? ".down" : ""), i.gain === null ? (i.value ? usd(i.value * i.quantity) : "—") : signedUsd(i.gain)),
      h("button", { type: "button", onclick: () => showSealed(i) }, "Edit…"),
      h("button", { type: "button", "aria-label": `Remove ${i.name}`, title: "Remove", onclick: () => remove(i) }, "✕")));
  fill($("#main"),
    h("div.section-head", h("h1", "Sealed product"), h("button.primary", { type: "button", onclick: () => showSealed() }, "Add sealed product")),
    h("p.muted", "Boxes, bundles and decks you keep unopened. Values are entered by hand (no free source prices sealed product) and aren't part of the cards' market value on Home."),
    h("div.tiles",
      tile("ITEMS", t.items.toLocaleString(), t.unvalued ? `${t.unvalued} without a value` : ""),
      tile("PRICE PAID", usd(t.paid), ""),
      tile("VALUE", usd(t.value), t.stale ? `${plural(t.stale, "value", "values")} to enter or check` : "every value is recent", "var(--blue)"),
      tile("GAIN / LOSS", g ? g.text : "—", g ? [g.pct, left ? `${plural(left, "item", "items")} without a price paid or value left out` : ""].filter(Boolean).join(" · ") : "needs prices paid and values", g ? g.color : null)),
    d.items.length ? h("div.card-box", d.items.map(row)) : h("p.muted", "No sealed product yet."));
}

// statLabel names a statistics row: a rarity, finish, condition, language or color.
function statLabel(id, key) {
  switch (id) {
    case "rarity": return title(key || "unknown");
    case "finish": return fname(key);
    case "condition": return choiceNames.default_condition[key] || key;
    case "language": return languageName(key);
    case "Color": return { W: "White", U: "Blue", B: "Black", R: "Red", G: "Green" }[key] || key;
    default: return key;
  }
}

// viewStats is the Statistics page.
async function viewStats() {
  const s = await api("GET", gameAPI("/stats"));
  const filter = (params) => () => go("collection", new URLSearchParams(params));
  const breakdown = (t) => {
    const max = Math.max(1, ...t.rows.map((r) => r.cards));
    return h("div.card-box",
      h("h2", { style: "margin-top:0" }, t.name),
      t.rows.length ? bars(t.rows.map((r) => ({
        label: statLabel(t.id, r.key),
        value: r.cards,
        text: `${r.cards.toLocaleString()} · ${usd(r.value)}`,
        onclick: t.param ? filter({ [t.param]: r.key }) : null,
      })), max, "var(--blue)") : h("p.muted", "No cards."));
  };
  const bandMax = Math.max(0.01, ...s.bands.map((b) => b.value));
  const bands = bars(s.bands.map((b) => {
    const params = {};
    if (b.min !== null) params.price_min = b.min;
    if (b.max !== null) params.price_max = (b.max - 0.01).toFixed(2);
    return {
      label: b.label,
      value: b.value,
      text: `${b.cards.toLocaleString()} · ${usd(b.value)} · ${Math.round(b.share * 100)}%`,
      onclick: b.label === "No price" ? filter({ unpriced: "1" }) : filter(params),
    };
  }), bandMax, "var(--amber)");
  // Tags count only the tagged copies, so they aren't a breakdown of whole cards.
  const tagsBox = (t) => {
    if (!t.tags.length) return null;
    const max = Math.max(1, ...t.tags.map((x) => x.copies));
    const rows = t.tags.map((x) => ({ label: x.name, value: x.copies, text: `${x.copies.toLocaleString()} · ${usd(x.value)}`,
      onclick: filter({ tags: x.name }) }));
    if (t.untagged) rows.push({ label: "Untagged", value: t.untagged.copies, text: `${t.untagged.copies.toLocaleString()} · ${usd(t.untagged.value)}`, onclick: filter({ untagged: "1" }) });
    return h("div.card-box", h("h2", { style: "margin-top:0" }, "Tags"), bars(rows, Math.max(max, t.untagged ? t.untagged.copies : 0), "var(--blue)"),
      h("p.note", "Copies carrying each tag, and their market value. A copy with two tags counts under both."));
  };
  const months = [...s.byMonth].reverse();
  const monthTable = (rows) => h("table.months",
    h("thead", h("tr", h("th", "Month"), h("th.n", "Copies"), h("th.n", "Spent"))),
    h("tbody", rows.map((m) => h("tr", h("td", m.month), h("td.n", m.cards.toLocaleString()), h("td.n", usd(m.paid))))));
  const monthBox = h("div.card-box.wide");
  const showMonths = (all) => fill(monthBox, monthTable(all ? months : months.slice(0, 12)),
    !all && months.length > 12 ? h("button.link", { type: "button", onclick: () => showMonths(true) }, `Show all ${months.length} months`) : null);
  showMonths(false);
  fill($("#main"),
    h("h1", "Statistics"),
    h("p.muted", "Counts are copies. Each bar's figures are copies and market value (and, by price, the share of the collection's value). Click a row to see those cards in the collection list."),
    h("h2", "By Price of One Copy"),
    h("div.card-box", bands),
    h("div.two-col.stats", s.breakdowns.map(breakdown), tagsBox(s.tags)),
    h("div.two-col",
      h("div", h("h2", "By Month Acquired"),
        months.length ? monthBox : h("p.muted", "No acquired dates recorded."),
        s.undated ? h("p.note", `${plural(s.undated, "copy has", "copies have")} no acquired date and aren't counted here.`) : null),
      h("div", h("h2", "Most Copies"),
        s.mostCopies.length ? h("div.card-box", cardRows(s.mostCopies, (c) => c.market_value)) : h("p.muted", "No card has more than one copy."))));
}

// compareNumber orders collector numbers: "2" < "10" < "10a" < "A1".
function compareNumber(a, b) {
  const na = parseInt(a, 10), nb = parseInt(b, 10);
  if (Number.isNaN(na) !== Number.isNaN(nb)) return Number.isNaN(na) ? 1 : -1;
  return (na - nb) || a.localeCompare(b);
}

// copyToList puts every missing card of a set on a new list (or an existing one).
async function copyToList(m, code) {
  const lists = await api("GET", gameAPI("/lists")).catch(() => []);
  const NEW = "\u0000new";
  const which = h("select", { "aria-label": "List", onchange: () => { name.hidden = which.value !== NEW; } },
    h("option", { value: NEW }, "A new list"), lists.map((l) => h("option", { value: l.id }, l.name)));
  const name = h("input", { type: "text", value: `${code} missing`, "aria-label": "New list's name" });
  const v = await askDialog("Copy to a list", [
    `Puts the ${plural(m.missing.length, "missing card", "missing cards")} on a list, one of each, as these printings.`, which, name,
  ], [{ label: "Copy", value: "copy", primary: true }]);
  if (!v) return;
  try {
    let id = which.value;
    if (id === NEW) {
      if (!name.value.trim()) { status("Name the new list.", true); return; }
      id = (await api("POST", gameAPI("/lists"), { name: name.value.trim() })).id;
    }
    const list = await api("GET", gameAPI(`/lists?id=${encodeURIComponent(id)}`));
    const entries = list.entries.map((e) => ({ card: e.card, quantity: e.quantity, any_printing: e.any_printing, finish: e.finish }))
      .concat(m.missing.map((c) => ({ card: c, quantity: 1, any_printing: false, finish: "" })));
    await api("PUT", gameAPI(`/lists?id=${encodeURIComponent(id)}`), { entries });
    status(`Copied ${plural(m.missing.length, "card", "cards")} to ${list.list.name}.`);
    go("lists", new URLSearchParams({ id }));
  } catch (e) { status(e.message, true); }
}

// viewTags is the Tags page: every tag with what carries it, to rename or delete.
async function viewTags() {
  const d = await api("GET", gameAPI("/tags"));
  const done = async (msg, r) => { status(msg, false, r.undo); await loadFilters(); route(); };
  const rename = async (t) => {
    const input = h("input", { type: "text", value: t.name, "aria-label": "New name" });
    const problem = h("p.error");
    const name = await new Promise((resolve) => {
      const ok = () => { const v = tidyTag(input.value); const why = tagProblem(v); if (why) { problem.textContent = why; return; } resolve(v); dlg.close(); };
      input.addEventListener("keydown", (e) => { if (e.key === "Enter") ok(); });
      const dlg = h("dialog.ask", h("div.dlg", h("header", h("h2", `Rename “${t.name}”`)),
        h("div.body", input, h("p.note", `It changes on ${plural(t.cards, "card", "cards")} (${plural(t.copies, "copy", "copies")}).`), problem),
        h("footer", h("button", { type: "button", onclick: () => { resolve(null); dlg.close(); } }, "Cancel"), h("button.primary", { type: "button", onclick: ok }, "Rename"))));
      dlg.addEventListener("close", () => { resolve(null); dlg.remove(); });
      document.body.append(dlg);
      dlg.showModal();
      input.select();
    });
    if (!name || name === t.name) return;
    const other = d.tags.find((x) => sameTag(x.name, name) && !sameTag(x.name, t.name));
    if (other && !(await confirmDialog(`“${other.name}” is already a tag. The two will be combined into one, “${other.name}”, on every card.`,
      { action: "Combine", title: "Combine the tags?" }))) return;
    try {
      const r = await api("PUT", gameAPI(`/tags?name=${encodeURIComponent(t.name)}`), { name });
      await done(other ? `Combined “${t.name}” into “${other.name}”.` : `Renamed “${t.name}” to “${name}”.`, r);
    } catch (e) { status(e.message, true); }
  };
  const remove = async (t) => {
    if (!(await confirmDialog(`Remove the tag “${t.name}” from ${plural(t.cards, "card", "cards")} (${plural(t.copies, "copy", "copies")})? The cards stay in your collection.`,
      { danger: true, action: "Delete tag", title: "Delete the tag?" }))) return;
    try {
      const r = await api("DELETE", gameAPI(`/tags?name=${encodeURIComponent(t.name)}`));
      await done(`Deleted the tag “${t.name}”.`, r);
    } catch (e) { status(e.message, true); }
  };
  // A row of the table; on a phone each row stacks, its figures labelled.
  const cells = (t) => [
    h("td.n", { "data-label": "Cards" }, t.cards.toLocaleString()), h("td.n", { "data-label": "Copies" }, t.copies.toLocaleString()),
    h("td.n", { "data-label": "Value" }, usd(t.value)), h("td.n", { "data-label": "Paid" }, usd(t.paid))];
  const row = (t) => h("tr",
    h("td.tag-name", h("a.tag-chip", { href: `#/collection?${new URLSearchParams({ tags: t.name })}`, style: tagStyle(t.name) }, t.name)),
    cells(t),
    h("td.actions", h("button", { type: "button", onclick: () => rename(t) }, "Rename…"), " ", h("button", { type: "button", onclick: () => remove(t) }, "Delete…")));
  const table = h("table.tags-table",
    h("thead", h("tr", h("th", "Tag"), h("th.n", "Cards"), h("th.n", "Copies"), h("th.n", "Market value"), h("th.n", "Price paid"), h("th", h("span.sr", "Actions")))),
    h("tbody", d.tags.map(row),
      d.untagged ? h("tr.untagged", h("td.tag-name", h("a", { href: "#/collection?untagged=1" }, "Untagged")), cells(d.untagged), h("td.actions")) : null));
  fill($("#main"),
    h("h1", "Tags"),
    h("p.muted", "Tags are labels on your copies, such as where they're kept or which deck they're in. Add them on a card's page, when adding cards, with Bulk edit, or to a whole import. Totals count only the tagged copies."),
    d.tags.length || d.untagged ? h("div.card-box", table) : h("p.muted", "No cards yet."),
    !d.tags.length && d.untagged ? h("p.muted", "No tags yet. Open a card and choose Add tags on one of its purchases.") : null);
}

// addInDialog shows a catalog card's add form in the dialog, leaving the page as it is.
function addInDialog(c, added) {
  const dlg = $("#dialog");
  const body = h("div.body");
  fill(dlg, h("div.dlg", h("header", h("h2", "Add a card")), body,
    h("footer", h("button", { type: "button", onclick: () => dlg.close() }, "Close"))));
  addForm(body, c, 0, () => { dlg.close(); if (added) added(); });
  dlg.showModal();
}

// cardImage is the card's picture, or a placeholder when images are off in Settings.
function cardImage(c, lazy) {
  if (!state.settings.show_images) return h("div.cardimg.off", "Card images are off (Settings)");
  return c.image_uri ? h("img.cardimg", { src: c.image_uri, alt: c.name, loading: lazy ? "lazy" : null }) : h("div.cardimg");
}

function openCard(key) {
  const p = location.hash.startsWith("#/collection") ? hashParams() : new URLSearchParams();
  p.set("key", key);
  // On a narrow screen the card replaces the list: Back returns to this spot.
  const narrow = matchMedia("(max-width: 1000px)").matches;
  if (narrow && location.hash.startsWith("#/collection")) state.listWindowY = window.scrollY;
  go("collection", p);
  if (narrow) window.scrollTo(0, 0);
}

function closeCard() {
  const p = hashParams();
  p.delete("key");
  go("collection", p);
}

// renderCard is the card editor: the card, its value, and "Your Copies", one row per
// purchase.
async function renderCard(container, key) {
  await loadLots();
  const d = await api("GET", gameAPI(`/card?key=${encodeURIComponent(key)}`));
  const c = d.card;
  const err = h("p.error");
  const conds = state.filters.conditions;
  // Only the finishes this printing comes in (plus any held, so none is ever lost).
  const finishes = [...new Set([...(c.finishes || []), ...c.purchases.map((l) => l.finish)].filter(Boolean))];
  if (!finishes.length) finishes.push(...state.filters.finishes);
  c.purchases.sort((a, b) => finishRank(a.finish) - finishRank(b.finish)); // stable: keeps condition and date order

  const rowsBox = h("div.purchases");
  const subtotal = h("p.muted");
  const rows = [];
  const update = () => {
    const by = new Map();
    let n = 0;
    for (const r of rows) {
      if (r.showSplit) r.showSplit();
      const q = parseInt(r.qty.value, 10) || 0;
      if (q <= 0) continue;
      n += q;
      const k = `${r.finish.value}|${r.cond.value}`;
      by.set(k, (by.get(k) || 0) + q);
    }
    const parts = [...by].map(([k, q]) => { const [f, cd] = k.split("|"); return `${fname(f)} ${cd} ×${q}`; });
    subtotal.textContent = n ? `${plural(n, "copy", "copies")}: ${parts.join(" · ")}` : "No copies: saving removes the card from your collection.";
  };
  // addRow adds a purchase row (after the row `after`, when splitting one).
  const addRow = (l, after) => {
    const r = {
      finish: h("select", { "aria-label": "Finish", onchange: update }, finishes.map((x) => h("option", { value: x, selected: x === l.finish }, fname(x)))),
      cond: h("select", { "aria-label": "Condition", onchange: update }, conds.map((x) => h("option", { value: x, selected: x === (l.condition || "NM") }, x))),
      qty: h("input", { type: "number", min: 0, step: 1, value: l.quantity ?? 1, "aria-label": "Quantity", oninput: update }),
      price: h("input", { type: "number", min: 0, step: "0.01", placeholder: "Paid each", value: l.price_paid || "", "aria-label": "Price paid each" }),
      date: h("input", { type: "date", value: l.acquired_date || "", "aria-label": "Acquired" }),
      note: h("input", { type: "text", value: l.note || "", placeholder: "Note: where they're kept, who they're for…", "aria-label": "Note" }),
      tags: tagEditor(l.tags, { label: "Tags on these copies" }),
    };
    r.lot = l.lot || "";
    // The note and the tags sit on their own line, folded away as "Add a note" and
    // "Add tags" until used.
    const noteBox = h("div.purchase-note");
    let noteShown = !!l.note, tagsShown = !!(l.tags && l.tags.length);
    // A purchase from a lot names it; the lot sets its price.
    const lotName = r.lot ? ((state.lots || []).find((x) => x.id === r.lot) || {}).name || "a lot" : "";
    const lotLine = r.lot ? h("p.note", "From ", h("a", { href: `#/lots?id=${encodeURIComponent(r.lot)}` }, lotName),
      ". The lot sets the price paid; a price typed here is replaced the next time the lot is repriced.") : null;
    // A saved purchase can be sold (as saved: unsaved edits to the row don't count).
    const sellButton = l.saved ? h("button.link", { type: "button", onclick: () => showSell(c, key, l) }, "Sell…") : null;
    // Split: one copy into a row of its own, to tag or note it apart from the rest.
    const splitButton = h("button.link", { type: "button", title: "Move one copy to a row of its own, to tag it apart", onclick: () => {
      const q = parseInt(r.qty.value, 10);
      if (!(q > 1)) return;
      let tags;
      try { tags = r.tags.value(); } catch { tags = []; }
      r.qty.value = q - 1;
      addRow({ finish: r.finish.value, condition: r.cond.value, quantity: 1, price_paid: parseFloat(r.price.value) || 0,
        acquired_date: r.date.value, note: r.note.value, lot: r.lot, tags }, r);
      rows[rows.indexOf(r) + 1].qty.focus();
    } }, "Split");
    r.showSplit = () => { splitButton.hidden = !(parseInt(r.qty.value, 10) > 1); };
    const showLine = () => fill(noteBox,
      noteShown ? r.note : h("button.link", { type: "button", onclick: () => { noteShown = true; showLine(); r.note.focus(); } }, "Add a note"),
      tagsShown ? r.tags.el : h("button.link", { type: "button", onclick: () => { tagsShown = true; showLine(); r.tags.focus(); } }, "Add tags"),
      splitButton, sellButton, lotLine);
    showLine();
    r.el = h("div.purchase-row",
      h("div.purchase", r.finish, r.cond, r.qty, r.price, r.date,
        h("button", { type: "button", title: "Remove this purchase", "aria-label": "Remove this purchase", onclick: () => {
          rows.splice(rows.indexOf(r), 1); r.el.remove(); update();
        } }, "✕")),
      noteBox);
    if (after) {
      rows.splice(rows.indexOf(after) + 1, 0, r);
      after.el.after(r.el);
    } else {
      rows.push(r);
      rowsBox.append(r.el);
    }
    update();
  };
  c.purchases.forEach((l) => addRow({ ...l, saved: true }));
  if (!c.purchases.length) addRow({ finish: finishes[0] });
  const addPurchase = h("button", { type: "button", onclick: () => {
    const last = rows[rows.length - 1];
    addRow({ finish: last ? last.finish.value : finishes[0], condition: last ? last.cond.value : state.settings.default_condition, quantity: 1, acquired_date: state.settings.default_date === "today" ? today() : "" });
    rows[rows.length - 1].qty.focus();
  } }, "+ Add purchase");

  const save = async () => {
    err.textContent = "";
    const purchases = [];
    for (const r of rows) {
      const quantity = parseInt(r.qty.value, 10);
      if (!Number.isInteger(quantity) || quantity < 0) { err.textContent = "Quantities must be whole numbers (0 or more)."; r.qty.focus(); return; }
      if (quantity === 0) continue;
      let tags;
      try { tags = r.tags.value(); } catch (e) { err.textContent = e.message; r.tags.focus(); return; }
      purchases.push({ finish: r.finish.value, condition: r.cond.value, quantity, price_paid: r.price.value ? parseFloat(r.price.value) : 0, acquired_date: r.date.value, note: r.note.value.trim(), lot: r.lot, tags });
    }
    if (!purchases.length && !(await ask(`Remove all of ${c.name} from your collection?`, "Remove"))) return;
    try {
      const r = await api("PUT", gameAPI(`/card?key=${encodeURIComponent(key)}`), { purchases });
      status(r.deleted ? `Deleted ${c.name}` : `Saved ${c.name}`);
      if (r.deleted) closeCard(); else openCard(r.key);
    } catch (e) { err.textContent = e.message; }
  };
  const del = async () => {
    if (!(await ask(`Remove all ${c.copy_count} of ${c.name} (${sn(c)}) from your collection?`, "Remove"))) return;
    try {
      await api("DELETE", gameAPI(`/card?key=${encodeURIComponent(key)}`));
      ticks().delete(key);
      status(`Deleted ${c.name}`);
      closeCard();
    } catch (e) { err.textContent = e.message; }
  };

  const printingsBox = h("div");
  const rawBox = h("pre.raw", { hidden: true }, JSON.stringify(d.raw, null, 2));
  const unpaid = c.purchases.filter((l) => !l.price_paid).reduce((n, l) => n + l.quantity, 0);
  const unpriced = c.purchases.filter((l) => !l.market).reduce((n, l) => n + l.quantity, 0);
  const g = c.market_value && c.price_paid ? gain(c.price_paid, c.market_value) : null;
  const marketText = Object.entries(c.market || {}).sort((a, b) => finishRank(a[0]) - finishRank(b[0]))
    .map(([k, v]) => `${usd(v)} ${fname(k)}`).join(" · ") + (c.prices_updated ? `  (as of ${c.prices_updated})` : "")
    + (englishPrice(c) ? " — the English printing's price: the catalog has none for this language" : "");
  const kv = [["Set", `${c.set_name} (${c.set})`], ["Collector number", c.collector_number], ["Rarity", title(c.rarity)],
    ...(d.details || []).map((r) => [r.Label, r.Value]),
    ["Language", languageName(c.lang || "en")],
    c.released_at ? ["Released", c.released_at] : null,
    ["Printed in", (c.finishes || []).map(fname).join(", ")], ["Market prices", marketText || "not available yet"]].filter(Boolean);
  const valueDet = [plural(c.copy_count, "copy", "copies"), unpriced ? `${unpriced} unpriced` : null].filter(Boolean).join(" · ");
  const paidDet = unpaid ? `${unpaid} without a price paid` : c.copy_count > 1 ? `${usd(c.price_paid / c.copy_count)} average` : "";

  fill(container, h("div.detail",
    h("button.back", { type: "button", onclick: closeCard }, "← Back"),
    h("h1", c.name),
    h("p.muted", [c.set_name, sn(c), title(c.rarity)].filter(Boolean).join(" · ")),
    h("div.detail-body",
      h("div", cardImage(c, true),
        d.link.url ? h("p", { style: "text-align:center" }, h("a", { href: d.link.url, target: "_blank", rel: "noopener noreferrer" }, d.link.label)) : null,
        h("p", { style: "text-align:center" }, h("button", { type: "button", onclick: () => showWatch(c) }, "Watch price…"))),
      h("div",
        h("div.tiles", { style: "grid-template-columns:repeat(auto-fit,minmax(140px,1fr))" },
          tile("MARKET VALUE", c.market_value ? usd(c.market_value) : "—", c.market_value ? [valueDet, englishPrice(c)].filter(Boolean).join(" · ") : "no market price yet", "var(--blue)"),
          tile("PRICE PAID", c.price_paid ? usd(c.price_paid) : "—", c.price_paid ? paidDet : "price paid not recorded"),
          tile("GAIN / LOSS", g ? g.text : "—", g ? g.pct : "needs both prices", g ? g.color : null)),
        h("h2", "Your Copies"),
        h("p.note", "One row per purchase, by finish and condition, with the price paid for each copy and an optional note. Copies with a different note stay a separate purchase."),
        h("div.purchase.heads", h("span", "Finish"), h("span", "Condition"), h("span", "Qty"), h("span", "Paid each"), h("span", "Acquired"), h("span")),
        rowsBox,
        h("p", addPurchase),
        subtotal,
        err,
        h("div.buttons", h("button.danger", { type: "button", onclick: del }, "Delete"),
          h("div.right", h("button", { type: "button", onclick: () => openCard(key) }, "Revert"), h("button.primary", { type: "button", onclick: save }, "Save changes"))),
        h("div.buttons",
          h("div.right", h("button", { type: "button", onclick: () => showPrintings(printingsBox, key, c) }, "Change printing…"),
            h("button", { type: "button", onclick: () => { rawBox.hidden = !rawBox.hidden; } }, "Raw data"))),
        printingsBox,
        rawBox,
        h("h2", "Card"),
        c.type_line ? h("p", h("strong", c.type_line)) : null,
        c.oracle_text ? h("p.rules", c.oracle_text) : null,
        h("dl.kv", kv.flatMap(([k, v]) => [h("dt", k), h("dd", v)])),
        d.others && d.others.length ? [h("h2", "Other Printings You Own"), h("div.rows", d.others.map((o) => h("button.row", { type: "button", onclick: () => openCard(o.key) },
          h("span.name", `${o.set_name} · ${sn(o)}`), h("span.where", copiesSummary(o)))))] : null))));
}

// showPrintings lists every printing of the card; picking one moves every copy there,
// keeping each purchase's finish, condition, quantity, price paid and date.
async function showPrintings(box, key, c, lang) {
  const language = lang || c.lang || "en";
  fill(box, h("p.muted", "Finding every printing…"));
  const languages = (state.filters.languages || []);
  const langMenu = languages.length ? h("label.inline", "Language ",
    h("select", { "aria-label": "Language", onchange: (e) => showPrintings(box, key, c, e.target.value) },
      languages.map((l) => h("option", { value: l.code, selected: l.code === language }, l.name)))) : null;
  try {
    const r = await api("GET", gameAPI(`/printings?key=${encodeURIComponent(key)}&lang=${encodeURIComponent(language)}`));
    fill(box, h("h2", "Change printing"),
      h("p.note", "Pick the printing you own; every purchase moves with its condition, price paid, date and note. A card's language belongs to its printing, so choose a language to see its printings."),
      langMenu,
      r.printings.length ? null : h("p.muted", `No printings in ${languageName(language)}.`),
      h("div.list.results", r.printings.map((p) => h("button.card" + (p.id === r.current ? ".sel" : ""), {
        type: "button",
        onclick: async () => {
          if (p.id === r.current) return;
          try {
            const res = await api("PUT", gameAPI(`/card/printing?key=${encodeURIComponent(key)}`), { id: p.id, lang: language });
            if (ticks().has(key)) { ticks().delete(key); ticks().set(res.key, tickInfo(p)); }
            status(`Moved ${c.name} to ${sn(p)}${language !== (c.lang || "en") ? ` (${languageName(language)})` : ""}${res.finishChanged ? ` (now ${res.finishes.map(fname).join(", ")}: the new printing doesn't come in every finish you had)` : ""}${res.merged ? ", merged with the copies you already had" : ""}`);
            await loadFilters();
            openCard(res.key);
          } catch (e) { status(e.message, true); }
        } },
        h("div.text", h("strong", `${p.set_name}${p.id === r.current ? "  (current)" : ""}`),
          h("span.meta", [sn(p), title(p.rarity), (p.released_at || "").slice(0, 4), (p.finishes || []).map(fname).join("/")].filter(Boolean).join(" · ")))))));
  } catch (e) { fill(box, langMenu, h("p.error", e.message)); }
}

// --- Add and Scan ------------------------------------------------------------

// addDefaults are the finish, condition, price and date of the last card added, so
// adding many cards from one purchase needs them typed once.
function addDefaults() {
  return state.addDefaults || { finish: "", condition: state.settings.default_condition, price: "", date: state.settings.default_date === "today" ? today() : "" };
}

// addCard adds copies of a catalog card and remembers the details for the next one.
async function addCard(c, d) {
  const r = await api("POST", gameAPI("/cards"), { card: c, quantity: d.quantity, finish: d.finish, condition: d.condition,
    price_paid: d.price ? parseFloat(d.price) : 0, acquired_date: d.date, lot: d.lot || "", tags: d.tags || [] });
  // Tags stay for the next card, so a stack going to one place is tagged once.
  if (!d.lot) state.addDefaults = { finish: d.finish, condition: d.condition, price: d.price, date: d.date, tags: d.tags };
  else state.addDefaults = { ...addDefaults(), finish: d.finish, condition: d.condition, tags: d.tags };
  await loadFilters();
  return r;
}

// addForm shows a catalog card with the details of the copies to add. After adding,
// onAdded(r, card, details) decides what happens next (by default, open the card).
// Returns { add } so a keyboard shortcut can add without clicking.
function addForm(container, c, owned, onAdded, lot) {
  const d0 = addDefaults();
  const qty = h("input", { type: "number", min: 1, step: 1, value: 1, "aria-label": "Quantity" });
  const finishes = c.finishes && c.finishes.length ? c.finishes : state.filters.finishes;
  const finish = h("select", { "aria-label": "Finish" }, finishes.map((x) => h("option", { value: x, selected: x === d0.finish }, fname(x))));
  const cond = h("select", { "aria-label": "Condition" }, state.filters.conditions.map((x) => h("option", { value: x, selected: x === d0.condition }, x)));
  const price = h("input", { type: "number", min: 0, step: "0.01", placeholder: "0.00", value: d0.price, "aria-label": "Price paid each" });
  const date = h("input", { type: "date", value: d0.date, "aria-label": "Acquired" });
  const tags = tagEditor(d0.tags, { label: "Tags" });
  const err = h("p.error");
  const add = async () => {
    const quantity = parseInt(qty.value, 10);
    if (!Number.isInteger(quantity) || quantity < 1) { err.textContent = "Quantity must be at least 1."; return; }
    let tagList;
    try { tagList = tags.value(); } catch (e) { err.textContent = e.message; return; }
    const details = { quantity, finish: finish.value, condition: cond.value, price: price.value, date: date.value, lot: lot ? lot.id : "", tags: tagList };
    try {
      const r = await addCard(c, details);
      status(`Added ${quantity}× ${c.name} (${sn(c)})${r.alreadyOwned ? " to the copies you already had" : ""}`);
      if (onAdded) onAdded(r, c, details); else openCard(r.key);
    } catch (e) { err.textContent = e.message; }
  };
  const price1 = anyPrice(c);
  fill(container, h("div.detail",
    h("h1", c.name), h("p.muted", `${c.set_name} — ${sn(c)}${price1 ? ` · ${usd(price1)}` : ""}`),
    owned ? h("p", { style: "color:var(--green)" }, `You own ${plural(owned, "copy", "copies")} of this printing.`) : null,
    h("div.detail-body",
      cardImage(c),
      h("div", c.type_line ? h("p", h("strong", c.type_line)) : null, c.oracle_text ? h("p.rules", c.oracle_text) : null,
        h("div.form", h("label", "Quantity"), qty, h("label", "Finish"), finish, h("label", "Condition"), cond,
          lot ? [h("label", "Price and date"), h("span.muted", `Set by the lot “${lot.name}”`)] : [h("label", "Price paid (each)"), price, h("label", "Acquired"), date],
          h("label", "Tags"), tags.el),
        err, h("div.buttons", h("div", h("button", { type: "button", onclick: () => addToList(c, parseInt(qty.value, 10) || 1) }, "Add to list…"), " ",
          h("button", { type: "button", onclick: () => showWatch(c, finish.value) }, "Watch…")),
          h("button.primary", { type: "button", onclick: add }, "Add to collection"))))));
  return { add };
}

// pickList lists catalog cards; picking one shows its add form in `preview`. The
// returned select(i) picks one from the keyboard.
function pickList(cards, owned, preview, onAdded, lot) {
  const picker = { current: null, index: -1 };
  const items = cards.map((c, i) => h("button.card", { type: "button", onclick: () => picker.select(i) },
    h("div.text", h("strong", c.name),
      h("span.meta", [sn(c), title(c.rarity), (c.released_at || "").slice(0, 4), anyPrice(c) ? usd(anyPrice(c)) : ""].filter(Boolean).join(" · "))),
    owned[i] ? h("span.owned", `own ${owned[i]}`) : null));
  picker.items = items;
  picker.select = (i) => {
    if (i < 0 || i >= items.length) return;
    items.forEach((x) => x.classList.remove("sel"));
    items[i].classList.add("sel");
    items[i].scrollIntoView({ block: "nearest" });
    picker.index = i;
    picker.current = addForm(preview, cards[i], owned[i], onAdded, lot);
  };
  return picker;
}

// Add: search the game's catalog, pick the exact printing, set ownership details.
// The page stays open after adding, so many cards can be added in a row; each is
// listed under "Added this session" with an Undo.
function viewAdd(params) {
  const pre = new URLSearchParams(params || "");
  const [textHint, setHint] = state.filters.searchHints;
  const text = h("input", { type: "search", placeholder: textHint, value: pre.get("text") || "", "aria-label": "Card name", autocomplete: "off" });
  const set = h("input.set", { type: "search", placeholder: setHint, value: pre.get("set") || "", "aria-label": "Set" });
  const suggest = h("ul.suggest", { hidden: true, role: "listbox" });
  const results = h("div.list.results", h("p.muted.empty", "Search by name, set code, or both, then pick a printing. ↑ ↓ move through the results; Enter adds."));
  const preview = h("div");
  const session = h("div.session");
  const banner = h("div");
  let next = "", cards = [], owned = [], total = 0, picker = null, armed = false;
  let lot = state.activeLot ? (state.lots || []).find((l) => l.id === state.activeLot && l.open) : null;
  // The lot being entered: what's in it so far, with Pause and Finish. Without one,
  // an open lot is offered again.
  const showBanner = () => {
    if (!lot) {
      const open = (state.lots || []).filter((l) => l.open);
      fill(banner, open.map((l) => h("div.card-box.notice.lot-banner", h("span", "An unfinished lot: ", h("strong", l.name), ` — ${lotProgress(l)}`),
        h("button", { type: "button", onclick: () => { state.activeLot = l.id; lot = l; showBanner(); refreshPicker(); } }, "Continue adding"))),
        h("p", h("button", { type: "button", onclick: () => showLotDialog(null, (l) => { state.activeLot = l.id; lot = l; showBanner(); refreshPicker(); }) }, "Add a lot…"),
          h("span.muted", " Cards bought together for one price, such as a bulk lot or a bundle. "), h("a", { href: "#/lots" }, "All lots")));
      return;
    }
    fill(banner, h("div.card-box.notice.lot-banner",
      h("div", h("strong", lot.name), h("div.muted", [lotProgress(lot), lot.split === "even" ? `${usd4(lot.priceEach)} each` : `worth ${usd(lot.marketValue)} so far`].join(" · ")),
        h("div", h("a", { href: `#/lots?id=${encodeURIComponent(lot.id)}` }, "This lot's page"), " · ", h("a", { href: "#/lots" }, "All lots"))),
      h("div.right", h("button", { type: "button", onclick: () => { state.activeLot = null; lot = null; showBanner(); refreshPicker(); } }, "Pause"),
        h("button.primary", { type: "button", onclick: () => finishLot(lot, () => { state.activeLot = null; lot = null; showBanner(); refreshPicker(); }) }, "Finish"))));
  };

  const showSession = () => {
    const added = state.added || [];
    fill(session, added.length ? [h("h2", "Added this session"), h("div.list", added.slice().reverse().map((a) => h("div.card",
      h("div.text", h("span.name", `${a.quantity}× ${a.name}`), h("span.meta", [`${a.set} · ${a.number}`, fname(a.finish), a.condition].join(" · "))),
      a.undone ? h("span.muted", "undone") : h("button", { type: "button", onclick: () => undoAdd(a) }, "Undo"))))] : null);
  };
  // undoAdd takes back exactly the copies one add put in: the same finish, condition,
  // price and date, less that quantity.
  const undoAdd = async (a) => {
    try {
      const d = await api("GET", gameAPI(`/card?key=${encodeURIComponent(a.key)}`));
      const price = a.price ? parseFloat(a.price) : 0;
      let left = a.quantity;
      const sameTags = (p) => (p.tags || []).length === (a.tags || []).length && (p.tags || []).every((t, i) => sameTag(t, a.tags[i]));
      const same = (p) => sameTags(p) && (a.lot ? p.lot === a.lot : p.price_paid === price && p.acquired_date === a.date && !p.lot);
      const purchases = d.card.purchases.map((p) => {
        if (left > 0 && p.finish === a.finish && p.condition === a.condition && same(p) && !p.note) {
          const take = Math.min(left, p.quantity);
          left -= take;
          return { ...p, quantity: p.quantity - take };
        }
        return p;
      }).filter((p) => p.quantity > 0);
      if (left > 0) throw new Error(`${a.name} has changed since it was added; change it on its card page instead.`);
      await api("PUT", gameAPI(`/card?key=${encodeURIComponent(a.key)}`), { purchases });
      if (a.lot) { // the lot's other cards are repriced without these copies
        const r = await api("PUT", gameAPI(`/lots?id=${encodeURIComponent(a.lot)}`), { recalculate: true });
        if (lot && lot.id === a.lot) { lot = r.lot; showBanner(); }
      }
      a.undone = true;
      status(`Took back ${a.quantity}× ${a.name}`);
      await loadFilters();
      showSession();
    } catch (e) { status(e.message, true); }
  };
  // After adding: stay here, ready for the next card.
  const onAdded = (r, c, d) => {
    (state.added = state.added || []).push({ key: r.key, name: c.name, set: c.set, number: c.collector_number, quantity: d.quantity,
      finish: d.finish, condition: d.condition, price: d.price, date: d.date, lot: d.lot, tags: d.tags || [] });
    if (r.lot) { lot = r.lot; showBanner(); }
    showSession();
    text.value = "";
    hideSuggest();
    armed = false;
    text.focus();
  };

  // refreshPicker rebuilds the results so their add form follows the active lot.
  const refreshPicker = () => { if (picker && cards.length) { const i = picker.index; render(); picker.select(Math.max(i, 0)); } };
  const render = () => {
    picker = pickList(cards, owned, preview, onAdded, lot);
    fill(results,
      h("p.muted.empty", cards.length ? (total > cards.length ? `Showing ${cards.length} of ${total} printings` : plural(total, "printing", "printings") + " found") : "No cards found."),
      picker.items,
      next ? h("button", { type: "button", style: "margin:8px 12px", onclick: () => run(true) }, "Load more results") : null);
  };
  const run = async (more) => {
    hideSuggest();
    if (!more) {
      if (!text.value.trim() && !set.value.trim()) { status("Enter a card name, a set code, or both."); return; }
      cards = []; owned = []; next = "";
    }
    status("Searching…");
    try {
      const p = new URLSearchParams({ text: text.value, set: set.value });
      if (more && next) p.set("next", next);
      const r = await api("GET", gameAPI(`/search?${p}`));
      cards = cards.concat(r.cards);
      owned = owned.concat(r.owned);
      next = r.next;
      total = r.total;
      render();
      status("");
      if (!more && cards.length) picker.select(0);
    } catch (e) { status(e.message, true); }
  };

  // Name suggestions while typing: after 2 characters, 200 ms after the last key, and
  // only the answer to the latest request is shown.
  let timer = null, asked = 0, chosen = -1;
  const hideSuggest = () => { clearTimeout(timer); asked++; suggest.hidden = true; chosen = -1; };
  const markSuggest = () => [...suggest.children].forEach((li, i) => li.classList.toggle("sel", i === chosen));
  const pickSuggest = (name) => { text.value = name; run(false); };
  text.addEventListener("input", () => {
    armed = false;
    clearTimeout(timer);
    const q = text.value.trim();
    if (q.length < 2 || /[:=<>!"]/.test(q)) { hideSuggest(); return; }
    timer = setTimeout(async () => {
      const mine = ++asked;
      try {
        const r = await api("GET", gameAPI(`/autocomplete?q=${encodeURIComponent(q)}`));
        if (mine !== asked) return; // a later request is on its way
        chosen = -1;
        fill(suggest, r.names.map((n) => h("li", { role: "option", onmousedown: (e) => { e.preventDefault(); pickSuggest(n); } }, n)));
        suggest.hidden = !r.names.length;
      } catch { /* suggestions are optional */ }
    }, 200);
  });
  text.addEventListener("blur", () => setTimeout(hideSuggest, 150));
  text.addEventListener("keydown", (e) => {
    const open = !suggest.hidden && suggest.children.length;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const step = e.key === "ArrowDown" ? 1 : -1;
      if (open) { chosen = Math.max(-1, Math.min(suggest.children.length - 1, chosen + step)); markSuggest(); return; }
      if (picker && picker.items.length) { picker.select(Math.max(0, Math.min(picker.items.length - 1, picker.index + step))); armed = true; }
    } else if (e.key === "Escape") {
      hideSuggest();
    } else if (e.key === "Enter") {
      if (open && chosen >= 0) { e.preventDefault(); pickSuggest(suggest.children[chosen].textContent); }
      else if (armed && picker && picker.current) { e.preventDefault(); picker.current.add(); }
      // otherwise the form submits: search
    }
  });

  // Paste a list: send it to the import preview.
  const listBox = h("textarea", { rows: 8, placeholder: "4 Lightning Bolt\n1 Sol Ring (C21) 263\n2 Shock [M19] *F*", "aria-label": "Card list" });
  const listOut = h("div");
  const listPanel = h("div.card-box", { hidden: true },
    h("p.muted", "One card per line: a count, the name, and optionally the set and collector number, as decklists from Moxfield, Archidekt or MTG Arena write them. Add *F* for foil."),
    listBox,
    h("div.buttons", h("div.right", h("button.primary", { type: "button", onclick: async () => {
      if (!listBox.value.trim()) { status("Paste a list first."); return; }
      status("Matching the cards…");
      try {
        const p = await whileRunning(api("POST", gameAPI("/import?preview=1"), new Blob([listBox.value], { type: "text/plain" })));
        status("");
        showImportPreview(listOut, p, lot ? `pasted list for ${lot.name}` : "pasted list", () => fill(listOut), lot,
          (r) => { if (r.lot) { lot = r.lot; showBanner(); } });
      } catch (e) { status(e.message, true); }
    } }, "Preview"))),
    listOut);
  const pasteBtn = h("button", { type: "button", onclick: () => { listPanel.hidden = !listPanel.hidden; if (!listPanel.hidden) listBox.focus(); } }, "Paste a list…");

  const form = h("form.search", { onsubmit: (e) => { e.preventDefault(); run(false); } },
    h("div.suggest-wrap", text, suggest), set, h("button.primary", { type: "submit" }, "Search"),
    pasteBtn, gameInfo().Scan ? h("a.button", { href: "#/scan" }, "Scan a card") : null);
  // The results keep the column to themselves; what's been added sits under the add form.
  fill($("#main"), h("h1", "Add Cards"), banner, form, listPanel, h("div.split", h("div.pane-list", results), h("div", preview, session)));
  showSession();
  loadLots().then(() => { if (lot) lot = (state.lots || []).find((l) => l.id === lot.id) || lot; showBanner(); });
  if (state.info.client !== "phone") text.focus(); // on phones the keyboard would cover the page
  if (pre.get("text") || pre.get("set")) run(false);
}

// Scan: point the camera at a card (or choose a photo); the text on it identifies the
// printing. The camera captures once two frames in a row read the same.
function viewScan() {
  if (!gameInfo().Scan) {
    fill($("#main"), h("h1", "Scan a Card"), h("p", "Card scanning reads the text on a card with Apple's Vision framework, so it works in the macOS and iPhone apps (and the web app when it runs on a Mac)."),
      h("p", h("a.button", { href: "#/add" }, "Search for a card instead")));
    return;
  }
  const say = h("div.say");
  const video = h("video", { playsinline: true, muted: true, autoplay: true });
  const camera = h("div.camera", { hidden: true }, video, h("div.guide"), say);
  const result = h("div");
  const file = h("input", { type: "file", accept: "image/*", hidden: true, onchange: () => {
    if (file.files.length) identify(file.files[0]);
    file.value = "";
  } });
  const startBtn = h("button.primary", { type: "button", onclick: () => startCamera() }, "Use the camera");
  const photoBtn = h("button", { type: "button", onclick: () => file.click() }, "Choose a photo…");
  const canCamera = !!(navigator.mediaDevices && navigator.mediaDevices.getUserMedia);

  const identify = async (blob) => {
    stopCamera();
    fill(result, h("p.muted.scan-read", "Reading the card…"));
    try {
      const r = await api("POST", gameAPI("/scan"), blob);
      const preview = h("div");
      // After adding a scanned card, go straight back to the camera for the next one.
      const again = () => { if (canCamera) startCamera(); else fill(result); };
      const picker = pickList(r.candidates, r.owned, preview, again);
      const best = r.candidates[0];
      let quick = null;
      if (r.exact && best) {
        const d = addDefaults();
        const fin = (best.finishes || []).includes(d.finish) ? d.finish : (best.finishes || [])[0] || "";
        quick = h("button.primary", { type: "button", onclick: async () => {
          quick.disabled = true;
          try {
            await addCard(best, { quantity: 1, finish: fin, condition: d.condition, price: d.price, date: d.date });
            status(`Added 1× ${best.name} (${sn(best)})`);
            again();
          } catch (e) { status(e.message, true); quick.disabled = false; }
        } }, `Add 1× ${best.name} (${sn(best)}) · ${fname(fin)} · ${d.condition}`);
      }
      fill(result,
        h("p.scan-read", "Read: ", h("strong", r.read)),
        r.note ? h("p.note.scan-read", r.note) : null,
        quick ? h("p.scan-read", quick, " ", h("span.muted", "or set the details below")) : null,
        r.candidates.length ? h("div.split",
          h("div.pane-list", h("div.list.results", picker.items)), preview)
          : h("p.scan-read", h("a.button", { href: `#/add?${new URLSearchParams({ text: r.name || "" })}` }, "Search by hand")));
      if (r.candidates.length) picker.select(0);
    } catch (e) { fill(result, h("p.error.scan-read", e.message)); }
  };

  let stream = null, timer = null;
  const stopCamera = () => {
    clearTimeout(timer);
    if (stream) stream.getTracks().forEach((t) => t.stop());
    stream = null;
    camera.hidden = true;
    startBtn.hidden = !canCamera;
  };
  state.scanStop = stopCamera;
  // frame grabs the camera's picture, at most 1920 pixels across: plenty for the
  // collector text, and far quicker to read than a 4K frame.
  const frame = () => new Promise((resolve) => {
    const scale = Math.min(1, 1920 / Math.max(video.videoWidth, video.videoHeight));
    const canvas = document.createElement("canvas");
    canvas.width = Math.round(video.videoWidth * scale);
    canvas.height = Math.round(video.videoHeight * scale);
    canvas.getContext("2d").drawImage(video, 0, 0, canvas.width, canvas.height);
    canvas.toBlob(resolve, "image/jpeg", 0.9);
  });
  const startCamera = async () => {
    fill(result);
    try {
      stream = await navigator.mediaDevices.getUserMedia({ video: { facingMode: "environment", width: { ideal: 1920 }, height: { ideal: 1440 } }, audio: false });
    } catch (e) {
      status(`The camera isn't available (${e.message}); choose a photo instead.`, true);
      return;
    }
    video.srcObject = stream;
    camera.hidden = false;
    startBtn.hidden = true;
    say.textContent = "Fill the frame with the card";
    let last = "";
    const tick = async () => {
      if (!stream) return;
      if (video.videoWidth) {
        const blob = await frame();
        try {
          const r = await api("POST", gameAPI("/scan/frame"), blob);
          if (!stream) return;
          if (r.complete && r.key === last) { identify(blob); return; }
          last = r.complete ? r.key : "";
          say.textContent = r.complete ? "Hold still…" : r.lines ? "Getting closer — keep the card flat and in the light" : "Fill the frame with the card";
        } catch (e) { say.textContent = e.message; }
      }
      timer = setTimeout(tick, 350);
    };
    tick();
  };

  fill($("#main"), h("h1", "Scan a Card"),
    h("p.muted", "Hold the card flat with its name and the bottom-left corner (set and collector number) in view. Cards from before 2014 are matched by name, so you pick the printing."),
    camera, h("div.scan-actions", canCamera ? startBtn : null, photoBtn, file), result);
  if (canCamera && gameInfo().Scan && state.info.client === "phone") startCamera();
}

// Search: owned cards matching q in every game, plus a catalog search to add more.
async function viewSearch(params) {
  const q = new URLSearchParams(params).get("q") || "";
  $("#gsq").value = q;
  const r = await api("GET", `/api/search?q=${encodeURIComponent(q)}`);
  const total = r.games.reduce((n, g) => n + g.Total, 0);
  const switchTo = async (id) => {
    if (state.game !== id) { state.game = id; store("game", id); $("#game").value = id; await loadFilters(); }
  };
  fill($("#main"), h("h1", `Search: “${q}”`),
    total ? null : h("p.muted", "No cards in your collection match. Search the catalog to add one."),
    r.games.map((g) => [
      r.games.length > 1 || g.Total ? h("h2", `${g.Name} — ${plural(g.Total, "match", "matches")}`) : null,
      g.Cards.length ? h("div.list", g.Cards.map((c) => h("button.card", { type: "button", onclick: async () => { await switchTo(g.Game); go("collection", new URLSearchParams({ key: c.key })); } },
        h("span.text", h("span.name", c.name), h("span.meta", `${sn(c)} · ${copiesSummary(c)}`))))) : null,
      g.Total > g.Cards.length ? h("p", h("span.muted", `Showing the first ${g.Cards.length}. `),
        h("button.link", { type: "button", onclick: async () => { await switchTo(g.Game); go("collection", new URLSearchParams({ q })); } }, `Show all ${g.Total.toLocaleString()}`)) : null,
      h("p", h("button", { type: "button", onclick: async () => { await switchTo(g.Game); go("add", new URLSearchParams({ text: q })); } },
        `Search the ${g.Name} catalog for “${q}”`)),
    ]));
}

// --- Tools and Settings ------------------------------------------------------

// Import: a CSV export from another app, or a collection .json from this app. The
// file is previewed first (matched, nothing saved), then imported on request.
function viewImport() {
  const file = h("input", { type: "file", accept: ".csv,.json,text/csv,application/json" });
  const out = h("div");
  const previewBtn = h("button.primary", { type: "button", onclick: async () => {
    if (!file.files.length) { status("Choose a file first."); return; }
    previewBtn.disabled = true;
    fill(out);
    status(`Reading ${file.files[0].name}… matching cards can take a minute for large files.`);
    try {
      const p = await whileRunning(api("POST", gameAPI("/import?preview=1"), file.files[0]));
      status("");
      showImportPreview(out, p, file.files[0].name, () => { file.value = ""; fill(out); });
    } catch (e) { status(e.message, true); }
    previewBtn.disabled = false;
  } }, "Preview");
  fill($("#main"),
    h("h1", "Import"),
    h("p", "Import a CSV export from another collection app (EchoMTG, ManaBox, Moxfield, Deckbox, TCGplayer, Dragon Shield, Archidekt and others), or a collection .json file exported from this app. Cards you already own add to their quantities."),
    h("p.muted", "You'll see what the file holds before anything is saved."),
    h("div.card-box", h("div.search", file, previewBtn)), out);
}

// unmatchedLink offers the rows that matched no card as a CSV with the file's header,
// to fix and import again.
function unmatchedLink(p, fileName) {
  const text = [p.header, ...p.unmatched.map((u) => u.text)].filter(Boolean).join("\n") + "\n";
  const url = URL.createObjectURL(new Blob([text], { type: "text/csv" }));
  const name = fileName.replace(/\.[^.]*$/, "") + "-unmatched.csv";
  return h("a.button", { href: url, download: name }, "Download the unmatched rows");
}

// showImportPreview shows what importing would do, with Import and Cancel.
function showImportPreview(out, p, fileName, cancel, lot, done) {
  const importTags = tagEditor([], { label: "Tag every card in this import" });
  const importBtn = h("button.primary", { type: "button", onclick: async () => {
    importBtn.disabled = true;
    status(`Importing ${plural(p.copies, "card", "cards")}…`);
    try {
      let tags;
      try { tags = importTags.value(); } catch (e) { status(e.message, true); importBtn.disabled = false; return; }
      const body = { token: p.token, tags };
      if (lot) body.lot = lot.id;
      const r = await whileRunning(api("POST", gameAPI("/import/commit"), body));
      status(`Imported ${plural(r.cards, "card", "cards")} from ${r.format}${lot ? ` into ${lot.name}` : ""}.`, false, r.undo);
      if (done) done(r);
      fill(out, h("h2", "Import Complete"), h("pre.summary", r.summary));
      await loadFilters();
    } catch (e) { status(e.message, true); importBtn.disabled = false; }
  } }, `Import ${plural(p.copies, "card", "cards")}`);
  const unmatched = p.unmatched.length;
  fill(out,
    h("h2", `Preview: ${fileName}`),
    h("p.muted", `${p.format} · ${plural(p.rows, "row", "rows")} read. Nothing has been saved yet.`),
    h("div.tiles",
      tile("CARDS", p.copies.toLocaleString(), plural(p.cards, "printing", "printings")),
      tile("NEW", p.newCards.toLocaleString(), "cards you don't own yet"),
      tile("ALREADY OWNED", p.alreadyOwned.toLocaleString(), "add to cards you own"),
      tile("PRICE PAID", usd(p.pricePaid), "for cards with a price")),
    unmatched ? h("p", h("strong", `${plural(unmatched, "row", "rows")} matched no card and won't be imported.`),
      " Download them, fix them, and import them again.") : null,
    unmatched ? h("p", unmatchedLink(p, fileName)) : null,
    unmatched ? h("details", h("summary", "Show the unmatched rows"),
      h("pre.raw", p.unmatched.map((u) => `Line ${u.line}: ${u.text}`).join("\n"))) : null,
    p.notes.length ? h("pre.summary", p.notes.join("\n")) : null,
    p.copies ? h("div.import-tags", h("label", "Tag every card in this import (optional)"), importTags.el) : null,
    h("div.buttons", h("div.right", p.copies ? importBtn : null, h("button", { type: "button", onclick: cancel }, "Cancel"))));
}

// Backups: the collection as it was before each import, bulk change and restore.
async function viewBackups() {
  const list = await api("GET", gameAPI("/backups"));
  const why = { import: "Before an import", "bulk-edit": "Before a bulk edit", "bulk-delete": "Before a bulk delete", restore: "Before a restore", merge: "Before a merge", move: "Before a move",
    "tag-rename": "Before renaming a tag", "tag-delete": "Before deleting a tag" };
  const restore = async (b, button) => {
    if (!(await confirmDialog(`Replace your collection with the backup from ${b.date} (${plural(b.cards, "unique card", "unique cards")})? Your current collection is backed up first, so you can come back to it.`, { action: "Restore" }))) return;
    button.disabled = true;
    try {
      const r = await api("POST", gameAPI("/backups/restore"), { name: b.name });
      status(`Restored the backup from ${b.date}: ${plural(r.cards, "unique card", "unique cards")}.`, false, r.undo);
      await loadFilters();
      await viewBackups();
    } catch (e) { status(e.message, true); button.disabled = false; }
  };
  fill($("#main"),
    h("h1", "Restore a Backup"),
    h("p", "A copy of the collection is saved before every import, bulk edit, bulk delete and restore. The last 10 are kept."),
    list.length ? h("div.card-box", list.map((b) => {
      const button = h("button", { type: "button", onclick: () => restore(b, button) }, "Restore");
      return toolRow(button, `${b.date} · ${why[b.reason] || b.reason} · ${plural(b.cards, "unique card", "unique cards")}`);
    })) : h("p.muted", "No backups yet. One is made before your first import or bulk change."));
}

// --- Lots ----------------------------------------------------------------------

// loadLots keeps the game's lots in state.lots (for names and the Add page).
async function loadLots() {
  try { state.lots = await api("GET", gameAPI("/lots")); } catch { state.lots = state.lots || []; }
  return state.lots;
}

// usd4 shows a lot's price per card to four decimal places, as the lot keeps it.
const usd4 = (v) => (v === null || v === undefined ? "—" : "$" + v.toFixed(4));

// lotProgress says how much of a lot has been entered: "37 of 500 entered".
const lotProgress = (l) => (l.expected !== null ? `${l.entered} of ${l.expected} entered` : `${plural(l.entered, "card", "cards")} entered`);

// lotCountText describes how a lot was counted.
function lotCountText(l) {
  const c = l.count;
  if (c.kind === "known") return `${plural(c.cards, "card", "cards")}, counted`;
  if (c.kind === "bundle") return `a bundle: ${c.packs} packs × ${c.cards_per_pack}${c.extra_cards ? ` + ${c.extra_cards} other cards` : ""}${c.lands ? ` + ${c.lands} basic lands${c.lands_share_cost ? "" : " (free)"}` : ""}`;
  return "counted as they're added";
}

// showLotDialog creates a lot (lot null) or edits one, then calls done(lot).
function showLotDialog(lot, done) {
  const l = lot || { name: "", total_paid: "", acquired_date: today(), count: { kind: "known", cards: "" }, split: "by_value", note: "" };
  const dlg = $("#dialog");
  const name = h("input", { type: "text", value: l.name, placeholder: "e.g. 500-card lot from eBay", "aria-label": "Name" });
  const total = h("input", { type: "number", min: 0, step: "0.01", value: l.total_paid, placeholder: "0.00", "aria-label": "Total paid" });
  const date = h("input", { type: "date", value: l.acquired_date, "aria-label": "Acquired" });
  const c = l.count;
  const kind = h("select", { "aria-label": "How many cards", onchange: () => { showKind(); preview(); } },
    h("option", { value: "known", selected: c.kind === "known" }, "I counted them"),
    h("option", { value: "bundle", selected: c.kind === "bundle" }, "It's a bundle"),
    h("option", { value: "as_entered", selected: c.kind === "as_entered" }, "I'll count as I add"));
  const num = (v, label, min) => h("input", { type: "number", min: min ?? 0, step: 1, value: v ?? "", "aria-label": label, oninput: () => preview() });
  const cards = num(c.kind === "known" ? c.cards : "", "Cards", 1);
  const packs = num(c.kind === "bundle" ? c.packs : 9, "Packs");
  const perPack = num(c.kind === "bundle" ? c.cards_per_pack : 14, "Cards per pack");
  const extra = num(c.kind === "bundle" ? c.extra_cards : 0, "Other cards");
  const lands = num(c.kind === "bundle" ? c.lands : 0, "Basic lands");
  const share = h("input", { type: "checkbox", checked: c.kind === "bundle" && c.lands_share_cost, onchange: () => preview() });
  const knownRows = h("div.form", h("label", "Cards"), cards);
  const bundleRows = h("div.form", h("label", "Packs"), packs, h("label", "Cards per pack"), perPack, h("label", "Other cards"), extra,
    h("label", "Basic lands"), lands, h("label", ""), h("label.check", share, "Lands share the cost"));
  const showKind = () => { knownRows.hidden = kind.value !== "known"; bundleRows.hidden = kind.value !== "bundle"; };
  const split = h("select", { "aria-label": "Split the cost", onchange: () => preview() },
    h("option", { value: "by_value", selected: l.split !== "even" }, "By each card's market value"),
    h("option", { value: "even", selected: l.split === "even" }, "Evenly"));
  const note = h("input", { type: "text", value: l.note || "", "aria-label": "Note" });
  const result = h("p.note");
  const err = h("p.error");
  const count = () => kind.value === "known" ? { kind: "known", cards: parseInt(cards.value, 10) || 0 }
    : kind.value === "bundle" ? { kind: "bundle", packs: parseInt(packs.value, 10) || 0, cards_per_pack: parseInt(perPack.value, 10) || 0,
      extra_cards: parseInt(extra.value, 10) || 0, lands: parseInt(lands.value, 10) || 0, lands_share_cost: share.checked }
    : { kind: "as_entered" };
  const preview = () => {
    const n = count();
    const divisor = n.kind === "known" ? n.cards : n.kind === "bundle" ? n.packs * n.cards_per_pack + n.extra_cards + (n.lands_share_cost ? n.lands : 0) : 0;
    const paid = parseFloat(total.value) || 0;
    if (split.value === "by_value") result.textContent = "Each card's price is worked out from its market value when you finish the lot. Until then, cards carry an even share.";
    else if (n.kind === "as_entered") result.textContent = "The total is divided by the cards entered, so every card's price changes as you add more.";
    else result.textContent = divisor > 0 ? `${plural(divisor, "card", "cards")}, $${(Math.round(paid / divisor * 10000) / 10000).toFixed(4)} each` : "";
  };
  [total].forEach((x) => x.addEventListener("input", preview));
  const save = async () => {
    err.textContent = "";
    const body = { name: name.value.trim(), total_paid: parseFloat(total.value) || 0, acquired_date: date.value, count: count(), split: split.value, note: note.value.trim() };
    try {
      const r = lot ? (await api("PUT", gameAPI(`/lots?id=${encodeURIComponent(lot.id)}`), body)).lot : await api("POST", gameAPI("/lots"), body);
      dlg.close();
      await loadLots();
      status(lot ? `Saved ${r.name}.` : `Started ${r.name}: cards you add now belong to it.`);
      done(r);
    } catch (e) { err.textContent = e.message; }
  };
  fill(dlg, h("div.dlg", h("header", h("h2", lot ? "Edit lot" : "Add a lot")),
    h("div.body",
      h("div.form", h("label", "Name"), name, h("label", "Total paid"), total, h("label", "Acquired"), date, h("label", "How many cards?"), kind),
      knownRows, bundleRows,
      h("div.form", h("label", "Split the cost"), split, h("label", "Note"), note),
      result, err),
    h("footer", h("button", { type: "button", onclick: () => dlg.close() }, "Cancel"), h("button.primary", { type: "button", onclick: save }, lot ? "Save" : "Start adding"))));
  showKind();
  preview();
  dlg.showModal();
  name.focus();
}

// finishLot finishes a lot: for a value split it first shows what each card will be
// costed at; for a short even-split lot it asks which count to divide by.
async function finishLot(lot, done) {
  if (lot.split === "by_value") {
    const pct = lot.marketValue > 0 ? Math.round(lot.total_paid / lot.marketValue * 1000) / 10 : null;
    const ok = await askDialog("Finish the lot", [`Paid ${usd(lot.total_paid)} for cards worth ${usd(lot.marketValue)} now.`,
      pct !== null ? `Each card will be costed at ${pct}% of its market price (prices from earlier days are refreshed first).` : "None of its cards has a market price, so the total will be split evenly."],
      [{ label: "Finish", value: true, primary: true }]);
    if (!ok) return;
  }
  try {
    const path = gameAPI(`/lots?id=${encodeURIComponent(lot.id)}`);
    const r = await api("PUT", path, { open: false });
    if (r.short && r.lot.split === "even") {
      const spread = await askDialog("Fewer cards than counted", [`You entered ${r.short.entered} of ${r.short.expected} cards. Spread ${usd(r.lot.total_paid)} over the ${r.short.entered} entered, or keep it at ${r.short.expected}?`],
        [{ label: `Keep ${r.short.expected}`, value: "keep" }, { label: `Spread over ${r.short.entered}`, value: "spread", primary: true }]);
      if (spread === "spread") await api("PUT", path, { count: { kind: "known", cards: r.short.entered } });
    }
    await loadLots();
    await loadFilters();
    status(`Finished ${r.lot.name}.${r.note ? " " + r.note : ""}${r.outcome && r.outcome.percent !== null ? ` Each card is costed at ${r.outcome.percent}% of its market price.` : ""}`);
    done(r.lot);
  } catch (e) { status(e.message, true); }
}

// Lots: each lot with what it cost and what its cards are worth now; with ?id=, one
// lot and its cards.
async function viewLots(params) {
  const id = new URLSearchParams(params).get("id");
  if (id) return viewLot(id);
  const lots = await loadLots();
  // A new lot opens the Add page ready to enter its cards.
  const addLot = () => showLotDialog(null, async (l) => {
    state.activeLot = l.id;
    await loadLots();
    go("add");
  });
  fill($("#main"), h("h1", "Lots"),
    h("p.muted", "Cards bought together for one price, such as a bulk lot or a bundle."),
    lots.length ? h("div.list", lots.map((l) => h("a.card", { href: `#/lots?id=${encodeURIComponent(l.id)}` },
      h("div.text", h("span.name", l.name, l.open ? h("span.muted", " · being entered") : null),
        h("span.meta", [l.acquired_date, `paid ${usd(l.total_paid)}`, lotProgress(l), `worth ${usd(l.marketValue)} now`].filter(Boolean).join(" · "))))))
      : h("p.muted", "No lots yet."),
    h("p", h("button.primary", { type: "button", onclick: addLot }, "Add a lot…")));
}

async function viewLot(id) {
  const d = await api("GET", gameAPI(`/lots?id=${encodeURIComponent(id)}`));
  const l = d.lot;
  const g = gain(l.total_paid, l.marketValue);
  const path = gameAPI(`/lots?id=${encodeURIComponent(id)}`);
  const continueAdding = async () => {
    try {
      if (!l.open) await api("PUT", path, { open: true });
      state.activeLot = l.id;
      await loadLots();
      go("add");
    } catch (e) { status(e.message, true); }
  };
  const recalc = async () => {
    try {
      const r = await api("PUT", path, { recalculate: true });
      status(`Repriced ${r.lot.name} at today's prices.${r.note ? " " + r.note : ""}`);
      route();
    } catch (e) { status(e.message, true); }
  };
  const del = async () => {
    const what = await askDialog(`Delete ${l.name}?`, ["Keep its cards (with the prices they have now), or delete them from your collection too?"],
      [{ label: "Delete the cards too", value: "remove", danger: true }, { label: "Keep the cards", value: "keep", primary: true }]);
    if (!what) return;
    try {
      const r = await api("DELETE", `${path}&cards=${what}`);
      status(`Deleted the lot ${l.name}${what === "remove" ? " and its cards" : ""}.`, false, r.undo);
      if (state.activeLot === l.id) state.activeLot = null;
      await loadFilters();
      go("lots");
    } catch (e) { status(e.message, true); }
  };
  const inLot = (c) => c.purchases.filter((p) => p.lot === id);
  fill($("#main"), h("p", h("a", { href: "#/lots" }, "← Lots")),
    h("h1", l.name),
    h("p.muted", [l.acquired_date, lotCountText(l), l.split === "even" ? "split evenly" : "split by market value", l.open ? "being entered" : l.split_date ? `split on ${l.split_date}` : ""].filter(Boolean).join(" · ")),
    l.note ? h("p", l.note) : null,
    h("div.tiles",
      tile("PAID", usd(l.total_paid), l.priceEach !== null ? `${usd4(l.priceEach)} each` : plural(l.copies, "card", "cards")),
      tile("WORTH NOW", usd(l.marketValue), plural(l.copies, "card", "cards"), "var(--blue)"),
      tile("CARDS", l.copies.toLocaleString(), lotProgress(l)),
      tile("GAIN / LOSS", g.text, g.pct, g.color)),
    h("div.buttons", h("div.right",
      h("button", { type: "button", onclick: () => showLotDialog(l, () => route()) }, "Edit"),
      h("button", { type: "button", onclick: continueAdding }, "Continue adding"),
      !l.open && l.split === "by_value" ? h("button", { type: "button", onclick: recalc }, "Recalculate at today's prices") : null,
      l.open ? h("button.primary", { type: "button", onclick: () => finishLot(l, () => route()) }, "Finish") : null,
      h("button.danger", { type: "button", onclick: del }, "Delete"))),
    h("h2", "Cards"),
    d.cards.length ? h("div.list", d.cards.map((c) => h("a.card", { href: `#/collection?key=${encodeURIComponent(c.key)}` },
      h("div.text", h("span.name", c.name),
        h("span.meta", inLot(c).map((p) => `${p.quantity}× ${fname(p.finish)} ${p.condition} at ${usd4(p.price_paid)}`).join(" · ") + ` · ${sn(c)}`))))) : h("p.muted", "No cards yet."));
}

// toolRow is one action on the Tools page with what it does.
const toolRow = (button, about) => h("div.tool", button, h("p.muted", about));

// exportCollection downloads (or, in apps that can't download, saves) an export:
// format "json" (the collection file), "csv" or "report", for the cards `params`
// filters (the collection list's parameters), or every card.
async function exportCollection(button, format = "json", params) {
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

// exportListed offers the collection list's cards as CSV or a valuation report.
async function exportListed(params, count) {
  const format = await askDialog("Export these cards", [
    `The ${plural(count, "card", "cards")} the list shows, in its order.`,
    "CSV opens in spreadsheets and other collection apps, and Import here reads it back. The valuation report is a page to print or save as PDF, with every card's value by set.",
  ], [{ label: "Valuation report", value: "report" }, { label: "CSV", value: "csv", primary: true }]);
  if (format) exportCollection(null, format, params);
}

async function viewTools() {
  const o = await api("GET", gameAPI("/home"));
  const s = state.settings;
  const every = { daily: "automatically each day", weekly: "automatically each week", manual: "only when you ask" }[s.price_refresh];
  const job = (label, path, busy, done) => h("button", { type: "button", onclick: (e) => runJob(e.currentTarget, path, busy, done) }, label);
  const where = state.info.client === "phone" ? "the app's folder in the Files app" : state.info.saveExports ? "your Downloads folder" : "your downloads";
  fill($("#main"),
    h("h1", "Tools"),
    state.info.games.length > 1 ? h("p.muted", `For ${o.name}. Choose a game in the sidebar to switch.`) : null,
    h("h2", "Collection"),
    h("div.card-box",
      toolRow(h("a.button", { href: "#/import" }, "Import…"), "Add cards from another app's CSV export (EchoMTG, ManaBox, Moxfield, Deckbox, TCGplayer and others) or from a collection file exported here."),
      toolRow(h("button", { type: "button", onclick: (e) => exportCollection(e.currentTarget) }, "Export collection"), `Save the collection file to ${where}, for a backup or to move to another device. Import reads it back.`),
      toolRow(h("button", { type: "button", onclick: (e) => exportCollection(e.currentTarget, "csv") }, "Export CSV"), `Save every card as a CSV file to ${where}, for spreadsheets and other collection apps: a row per purchase, with its price paid, date, note and market price. Import reads it back. To export only some cards, filter the collection list and choose Export there.`),
      toolRow(h("button", { type: "button", onclick: (e) => exportCollection(e.currentTarget, "report") }, "Valuation report"), `Save a page listing every card by set with its value, to ${where}. Open it in a browser to print it or save it as a PDF, for insurance or your records.`),
      toolRow(h("button", { type: "button", onclick: () => { go("collection"); setTimeout(showBulkEdit, 50); } }, "Bulk edit…"),
        "Change the condition, finish, price paid or date of many cards at once, or delete them. Tick the cards in the collection list (filter it, then Select all shown); Bulk edit is also below the list."),
      toolRow(h("a.button", { href: "#/tags" }, "Tags…"), "Every tag on your cards, with how many cards and copies carry it and what they're worth. Rename or delete a tag everywhere at once."),
      toolRow(h("a.button", { href: "#/stats" }, "Statistics…"), "Your collection broken down by rarity, finish, condition, language, release year, color and card type, by price band, and by month acquired."),
      toolRow(h("a.button", { href: "#/lists" }, "Lists…"), "Decks and want lists: the cards you want, how many you own already, and what the rest would cost."),
      toolRow(h("a.button", { href: "#/sealed" }, "Sealed product…"), "Boxes, bundles and decks you keep unopened: what you paid and what they're worth. There's no free price source for sealed product, so you enter each value yourself."),
      toolRow(h("a.button", { href: "#/watchlist" }, "Watchlist…"), "Cards whose price you're watching, with an alert when one moves by a percentage or crosses a price. Prices are checked when the app refreshes them, so alerts appear while the app is open or the next time you open it."),
      toolRow(h("a.button", { href: "#/earnings" }, "Earnings…"), "Cards you've sold: what they sold for, what they cost, fees, and the gain, by month and year. Sell copies with Sell… on a card's purchase."),
      toolRow(h("a.button", { href: "#/lots" }, "Lots…"), "Cards bought together for one price, such as a bulk lot or a bundle: what each lot cost and what its cards are worth now."),
      toolRow(h("a.button", { href: "#/backups" }, "Restore a backup…"), "Go back to the collection as it was before a recent import, bulk edit or bulk delete. The last 10 backups are kept."),
      gameInfo().Scan ? toolRow(h("a.button", { href: "#/scan" }, "Scan a card…"), "Identify a card from the camera or a photo, then add it.") : null),
    h("h2", "Prices"),
    h("div.card-box",
      toolRow(job("Refresh prices now", "/refresh", "Refreshing prices…", (r) => `Prices updated for ${plural(r.updated, "card", "cards")}`),
        `Fetch today's market prices now. ${o.pricesAsOf ? `Prices are from ${o.pricesAsOf}` : "No prices have been fetched yet"}; they update ${every} (see Settings).`),
      o.canLoadHistory ? toolRow(job(o.historyLoaded ? "Update price history" : "Load price history", "/price-history", "Loading price history… this downloads about 50 MB.", (r) => `Loaded price history for ${plural(r.printings, "printing", "printings")}`),
        o.historyLoaded ? `Price history last loaded ${o.historyLoaded}. Download ${o.historyInfo} again to fill in recent days.`
          : `Download ${o.historyInfo} so the value chart shows the past, not just the days the app was running.`) : null));
}

const choiceNames = {
  theme: { system: "Match system", light: "Light", dark: "Dark" },
  text_size: { small: "Small", normal: "Normal", large: "Large", larger: "Larger" },
  card_order: { name: "By name", newest: "By set, newest first", oldest: "By set, oldest first", value: "Most valuable first" },
  set_completion: { main: "The printed set (e.g. 280 cards)", all: "Every card Scryfall lists (variants too)" },
  chart_range: { "30D": "30 days", "90D": "90 days", "1Y": "1 year", All: "All time" },
  price_refresh: { daily: "Daily", weekly: "Weekly", manual: "Only when I ask" },
  default_condition: { NM: "NM — Near Mint", LP: "LP — Lightly Played", MP: "MP — Moderately Played", HP: "HP — Heavily Played", DMG: "DMG — Damaged" },
  default_date: { today: "Today", none: "Leave blank" },
  log_level: { debug: "Debug (most detail)", info: "Info", warn: "Warnings", error: "Errors only" },
};

// saveSettings sends changed settings, then applies what was saved.
async function saveSettings(change, what) {
  try {
    const r = await api("PUT", "/api/settings", change);
    state.settings = r.settings;
    applySettings();
    status(`Saved: ${what}`);
  } catch (e) { status(e.message, true); }
}

// applySettings applies the appearance settings to the page.
function applySettings() {
  const s = state.settings;
  const root = document.documentElement;
  if (s.theme === "system") delete root.dataset.theme; else root.dataset.theme = s.theme;
  root.style.setProperty("--scale", { small: 0.9, normal: 1, large: 1.15, larger: 1.3 }[s.text_size] || 1);
}

async function viewSettings() {
  const d = await api("GET", "/api/settings");
  state.settings = d.settings;
  const s = d.settings;
  const menu = (key, what) => h("select", { id: `set-${key}`, onchange: (e) => saveSettings({ [key]: e.target.value }, what) },
    d.choices[key].map((v) => h("option", { value: v, selected: v === s[key] }, (choiceNames[key] || {})[v] || v)));
  const check = (key, label, what) => h("label.check", h("input", { type: "checkbox", id: `set-${key}`, checked: s[key],
    onchange: (e) => saveSettings({ [key]: e.target.checked }, what) }), label);
  const row = (label, control, hint) => [h("label", { for: control.id || null }, label), h("div", control, hint ? h("div.hint", hint) : null)];
  const section = (name, ...rows) => [h("h2", name), h("div.card-box", h("div.form.settings", { style: "margin:0" }, rows.flat()))];
  const phone = state.info.client === "phone";
  fill($("#main"),
    h("h1", "Settings"),
    h("p.muted", "Changes are saved right away and apply to every game."),
    section("Appearance",
      row("Theme", menu("theme", "theme"), "Light, dark, or follow the system setting"),
      row("Text size", menu("text_size", "text size"), "Scales text and spacing"),
      row("Card images", check("show_images", "Show card images", "card images"), "Turn off to save data on slow or metered connections")),
    section("Collection",
      row("Card order", menu("card_order", "card order"), "How cards are listed in the collection, until you choose a Sort there"),
      row("Set completion", menu("set_completion", "set completion"), "What a set's completion counts: the cards numbered up to the size printed on them, or every card including variants, promos and extras"),
      row("Value chart", menu("chart_range", "chart range"), "The range the Value Over Time chart shows first")),
    section("Prices",
      row("Update prices", menu("price_refresh", "price updates"), "You can also refresh them on the Tools page"),
      row("Price history", check("auto_price_history", "Load price history automatically", "price history"), "The first time, on desktop and web (about a 50 MB download); phones ask first")),
    section("Adding Cards",
      row("Condition", menu("default_condition", "default condition"), "Preselected when adding a card"),
      row("Acquired date", menu("default_date", "default date"), "Filled in when adding a card")),
    section("Advanced",
      row("Deleting", check("confirm_delete", "Ask before deleting cards", "delete confirmation")),
      row("Log detail", menu("log_level", "log level"), phone ? "How much goes in the log file" : "How much goes in the log file; --log-level overrides it at startup")),
    h("h2", "Data"),
    dataSection(await dataFolder(), d.dataDir),
    h("h2", "About"),
    h("p", `${state.info.name} ${state.info.version}`),
    h("p", h("button", { type: "button", onclick: async () => {
      if (!(await confirmDialog("Put every setting back to how it was when the app was installed? Your collection isn't affected.", { action: "Restore defaults" }))) return;
      await saveSettings(d.defaults, "defaults restored");
      route();
    } }, "Restore defaults")));
}

// --- Routing -----------------------------------------------------------------

function go(view, params, replace) {
  const qs = params && [...params.keys()].length ? `?${params}` : "";
  const hash = `#/${view}${qs}`;
  // Setting the same hash fires no hashchange (e.g. reopening a card after saving it).
  if (replace || location.hash === hash) { history.replaceState(null, "", hash); route(); } else location.hash = hash;
}

async function loadFilters() {
  state.filters = await api("GET", gameAPI("/filters"));
}

async function route() {
  const [path, qs] = location.hash.replace(/^#\/?/, "").split("?");
  const view = path || "home";
  if (state.scanStop && view !== "scan") { state.scanStop(); state.scanStop = null; }
  const active = ["import", "backups", "lots", "earnings", "watchlist", "sealed", "tags"].includes(view) ? "tools" : view === "scan" ? "add" : view;
  document.querySelectorAll("#nav a").forEach((a) => a.classList.toggle("active", a.dataset.view === active));
  // The tab bar has no Lists or Statistics tab: they're reached from Tools there.
  document.querySelectorAll(".tabbar a").forEach((a) => a.classList.toggle("active", a.dataset.view === (["lists", "stats"].includes(active) ? "tools" : active)));
  try {
    if (view === "collection") await viewCollection(qs || "");
    else if (view === "add") viewAdd(qs || "");
    else if (view === "scan") viewScan();
    else if (view === "search") await viewSearch(qs || "");
    else if (view === "import") viewImport();
    else if (view === "backups") await viewBackups();
    else if (view === "lots") await viewLots(qs || "");
    else if (view === "earnings") await viewEarnings(qs || "");
    else if (view === "lists") await viewLists(qs || "");
    else if (view === "watchlist") await viewWatchlist();
    else if (view === "sealed") await viewSealed();
    else if (view === "stats") await viewStats();
    else if (view === "tags") await viewTags();
    else if (view === "tools") await viewTools();
    else if (view === "settings") await viewSettings();
    else await viewHome();
  } catch (e) {
    fill($("#main"), h("p.error", e.message));
  }
}

// Background work (the daily price refresh, a load in another window) changes the
// data; the open page refreshes unless the user is in the middle of something.
async function watchForChanges() {
  try {
    const { revision } = await api("GET", "/api/status");
    const busy = document.activeElement && ["INPUT", "SELECT", "TEXTAREA"].includes(document.activeElement.tagName);
    if (state.skipRevision || state.revision === null) {
      state.revision = revision;
      state.skipRevision = false;
    } else if (revision !== state.revision && !busy && !document.querySelector("dialog[open]") && !location.hash.startsWith("#/scan") && !location.hash.startsWith("#/add")) {
      state.revision = revision;
      await loadFilters();
      route();
    }
  } catch { /* the app may be closing */ }
}

function showGame() {
  const g = gameInfo();
  document.querySelectorAll(".game-name").forEach((el) => { el.textContent = shortName(g.Name); });
  document.querySelectorAll("[data-scan]").forEach((el) => { el.hidden = !g.Scan; });
}

async function init() {
  state.info = await api("GET", "/api/info");
  state.settings = (await api("GET", "/api/settings")).settings;
  applySettings();
  state.range = state.settings.chart_range;
  document.documentElement.dataset.client = state.info.client;
  const saved = store("game");
  state.game = state.info.games.some((g) => g.ID === saved) ? saved : state.info.games[0].ID;
  const sel = $("#game");
  fill(sel, ...state.info.games.map((g) => h("option", { value: g.ID, selected: g.ID === state.game }, g.Name)));
  sel.hidden = state.info.games.length < 2;
  sel.addEventListener("change", async () => { state.game = sel.value; store("game", sel.value); showGame(); await loadFilters(); go("home"); });
  $("#globalsearch").addEventListener("submit", (e) => { e.preventDefault(); const q = $("#gsq").value.trim(); if (q) go("search", new URLSearchParams({ q })); });
  $("#version").textContent = `${state.info.name} ${state.info.version}`;
  document.title = state.info.name;
  showGame();
  await loadFilters();
  window.addEventListener("hashchange", route);
  // In the apps, the web view drops links that open a new window (View on Scryfall):
  // links to other sites open in the system's browser instead.
  document.addEventListener("click", (e) => {
    const a = e.target.closest && e.target.closest("a[href]");
    if (!a || !state.info.opensLinks || !/^https?:/i.test(a.href) || new URL(a.href).origin === location.origin) return;
    e.preventDefault();
    api("POST", "/api/open", { url: a.href }).catch((err) => status(err.message, true));
  });
  setInterval(() => { if (!document.hidden) watchForChanges(); }, 15000);
  document.addEventListener("visibilitychange", () => { if (!document.hidden) watchForChanges(); });
  route();
}

init().catch((e) => fill($("#main"), h("p.error", `Could not start: ${e.message}`)));
