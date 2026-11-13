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

function status(msg, isError) {
  const el = $("#status");
  el.textContent = msg || "";
  el.className = isError ? "error" : "";
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

// ask confirms a deletion, unless the user turned that off in Settings.
const ask = (msg) => !state.settings.confirm_delete || confirm(msg);

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
    it.onclick ? h("div.label.link", { title: it.label, onclick: it.onclick, role: "link" }, it.label) : h("div.label", { title: it.label }, it.label),
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
  return h("div.rows", cards.map((c, i) => h("div.row", { onclick: () => openCard(c.key) },
    h("span.name", `${i + 1}. ${c.name}`),
    h("span.where", `${sn(c)} · ${copiesSummary(c)}`),
    h("span.amt", usd(amount(c))))));
}

// --- Home --------------------------------------------------------------------

async function viewHome() {
  const range = state.range;
  const o = await api("GET", gameAPI(`/home?range=${encodeURIComponent(range)}`));
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
    h("div.tiles",
      tile("CARDS", o.cards.toLocaleString(), `${plural(o.uniqueCards, "unique card", "unique cards")} · ${plural(o.sets, "set", "sets")}`),
      tile("PRICE PAID", usd(o.pricePaid), o.pricePaidUnknown ? `${plural(o.pricePaidUnknown, "card", "cards")} without a price paid` : "every card has a price paid"),
      tile("MARKET VALUE", usd(o.marketValue), `prices as of ${o.pricesAsOf || "never"}${o.unpriced ? ` · ${o.unpriced} unpriced` : ""}`, "var(--blue)"),
      tile("GAIN / LOSS", g.text, g.pct, g.color)),
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
    const r = await api("POST", gameAPI(path));
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
const tickInfo = (c) => ({ name: c.name, where: sn(c) });

// The collection: filters and every matching card (each with a tick box for bulk
// edit), and beside them the selected card's editor or, when a set is chosen, the
// set's page.
async function viewCollection(params) {
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
  const filters = h("div.filters", search, h("div.menus",
    menu("set", "Set", (f.sets || []).map((x) => ({ value: x.Code, label: x.Label }))),
    (f.filters || []).map((x) => menu("f." + x.Name, x.Name, x.Options)),
    menu("rarity", "Rarity", f.rarities, title),
    menu("finish", "Finish", f.finishes, fname)));

  const apiQ = new URLSearchParams(q);
  apiQ.delete("key");
  const data = await api("GET", gameAPI(`/cards?${apiQ}`));
  const filtering = [...apiQ.keys()].length > 0;
  const ticked = ticks();
  if (!filtering) { // every card is listed: drop ticks for cards no longer owned
    const keys = new Set(data.cards.map((c) => c.key));
    for (const k of [...ticked.keys()]) if (!keys.has(k)) ticked.delete(k);
  }

  const boxes = new Map();
  const row = (c) => {
    const box = h("input", { type: "checkbox", checked: ticked.has(c.key), "aria-label": `Tick ${c.name} for bulk edit`,
      onclick: (e) => e.stopPropagation(),
      onchange: (e) => { e.target.checked ? ticked.set(c.key, tickInfo(c)) : ticked.delete(c.key); updateBar(); } });
    boxes.set(c.key, box);
    return h("div.card" + (c.key === q.get("key") ? ".sel" : ""), { onclick: () => openCard(c.key) },
      box, h("div.text", h("span.name", c.name), h("span.meta", `${sn(c)} · ${copiesSummary(c)}`)));
  };
  const list = h("div.list", data.cards.length ? data.cards.map(row) : h("p.muted.empty", "No cards match."));

  const counts = `${plural(data.uniqueCards, "unique card", "unique cards")} · ${plural(data.cardCount, "card", "cards")} · ${plural(data.sets, "set", "sets")}`;
  const clear = filtering ? h("button.link", { type: "button", onclick: () => go("collection") }, "Clear filters") : null;

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
  const split = h("div.split", h("div.pane-list", filters, h("p.counts.muted", h("span", filtering ? `Matching: ${counts}` : counts), clear), list, bar), detail);
  fill($("#main"), split);
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
  const del = h("input", { type: "checkbox" });
  const err = h("p.error");
  const apply = async () => {
    err.textContent = "";
    if (!chosen.size) { err.textContent = "Tick at least one card."; return; }
    const body = { keys: [...chosen.keys()], condition: cond.value, finish: fin.value, acquired_date: date.value, delete: del.checked };
    if (price.value !== "") body.price_paid = parseFloat(price.value);
    if (!body.delete && !body.condition && !body.finish && !body.acquired_date && body.price_paid === undefined) { err.textContent = "Choose at least one change."; return; }
    const n = plural(chosen.size, "card", "cards");
    if (!(body.delete ? ask(`Delete every copy of ${n}? This can't be undone.`) : confirm(`Apply the changes to every copy of ${n}?`))) return;
    try {
      const r = await api("POST", gameAPI("/bulk"), body);
      status(body.delete ? `Deleted ${plural(r.Deleted, "card", "cards")}` :
        `Updated ${plural(r.Updated, "card", "cards")}${r.Skipped.length ? `; ${r.Skipped.length} kept their finish (the printing doesn't come in it)` : ""}`);
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
        h("label", ""), h("label.check", { style: "display:flex;gap:8px;align-items:center;color:var(--red);text-align:left" }, del, "Delete these cards")),
      h("p.note", "Changes apply to every copy of each card above. Finish changes skip printings that don't come in that finish; purchases that end up the same combine."),
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
      fill(missingBox,
        h("p.muted.empty", m.missing.length ? `${plural(m.missing.length, "card", "cards")} you don't own yet. Click one to add it.` : "You have every card in this set."),
        m.missing.map((c) => h("div.card", { onclick: () => go("add", new URLSearchParams({ text: c.name, set: code })) },
          h("span.num", c.collector_number), h("div.text", h("span.name", c.name),
            h("span.meta", [title(c.rarity), anyPrice(c) ? usd(anyPrice(c)) : ""].filter(Boolean).join(" · "))))));
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

// cardImage is the card's picture, or a placeholder when images are off in Settings.
function cardImage(c, lazy) {
  if (!state.settings.show_images) return h("div.cardimg.off", "Card images are off (Settings)");
  return c.image_uri ? h("img.cardimg", { src: c.image_uri, alt: c.name, loading: lazy ? "lazy" : null }) : h("div.cardimg");
}

function openCard(key) {
  const p = location.hash.startsWith("#/collection") ? hashParams() : new URLSearchParams();
  p.set("key", key);
  go("collection", p);
  if (matchMedia("(max-width: 1000px)").matches) window.scrollTo(0, 0); // the card replaces the list
}

function closeCard() {
  const p = hashParams();
  p.delete("key");
  go("collection", p);
}

// renderCard is the card editor: the card, its value, and "Your Copies", one row per
// purchase.
async function renderCard(container, key) {
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
      const q = parseInt(r.qty.value, 10) || 0;
      if (q <= 0) continue;
      n += q;
      const k = `${r.finish.value}|${r.cond.value}`;
      by.set(k, (by.get(k) || 0) + q);
    }
    const parts = [...by].map(([k, q]) => { const [f, cd] = k.split("|"); return `${fname(f)} ${cd} ×${q}`; });
    subtotal.textContent = n ? `${plural(n, "copy", "copies")}: ${parts.join(" · ")}` : "No copies: saving removes the card from your collection.";
  };
  const addRow = (l) => {
    const r = {
      finish: h("select", { "aria-label": "Finish", onchange: update }, finishes.map((x) => h("option", { value: x, selected: x === l.finish }, fname(x)))),
      cond: h("select", { "aria-label": "Condition", onchange: update }, conds.map((x) => h("option", { value: x, selected: x === (l.condition || "NM") }, x))),
      qty: h("input", { type: "number", min: 0, step: 1, value: l.quantity ?? 1, "aria-label": "Quantity", oninput: update }),
      price: h("input", { type: "number", min: 0, step: "0.01", placeholder: "Paid each", value: l.price_paid || "", "aria-label": "Price paid each" }),
      date: h("input", { type: "date", value: l.acquired_date || "", "aria-label": "Acquired" }),
    };
    r.el = h("div.purchase", r.finish, r.cond, r.qty, r.price, r.date,
      h("button", { type: "button", title: "Remove this purchase", "aria-label": "Remove this purchase", onclick: () => {
        rows.splice(rows.indexOf(r), 1); r.el.remove(); update();
      } }, "✕"));
    rows.push(r);
    rowsBox.append(r.el);
    update();
  };
  c.purchases.forEach(addRow);
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
      purchases.push({ finish: r.finish.value, condition: r.cond.value, quantity, price_paid: r.price.value ? parseFloat(r.price.value) : 0, acquired_date: r.date.value });
    }
    if (!purchases.length && !ask(`Remove all of ${c.name} from your collection?`)) return;
    try {
      const r = await api("PUT", gameAPI(`/card?key=${encodeURIComponent(key)}`), { purchases });
      status(r.deleted ? `Deleted ${c.name}` : `Saved ${c.name}`);
      if (r.deleted) closeCard(); else openCard(r.key);
    } catch (e) { err.textContent = e.message; }
  };
  const del = async () => {
    if (!ask(`Remove all ${c.copy_count} of ${c.name} (${sn(c)}) from your collection?`)) return;
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
    .map(([k, v]) => `${usd(v)} ${fname(k)}`).join(" · ") + (c.prices_updated ? `  (as of ${c.prices_updated})` : "");
  const kv = [["Set", `${c.set_name} (${c.set})`], ["Collector number", c.collector_number], ["Rarity", title(c.rarity)],
    ...(d.details || []).map((r) => [r.Label, r.Value]),
    c.lang && c.lang !== "en" ? ["Language", c.lang.toUpperCase()] : null,
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
        d.link.url ? h("p", { style: "text-align:center" }, h("a", { href: d.link.url, target: "_blank", rel: "noopener noreferrer" }, d.link.label)) : null),
      h("div",
        h("div.tiles", { style: "grid-template-columns:repeat(auto-fit,minmax(140px,1fr))" },
          tile("MARKET VALUE", c.market_value ? usd(c.market_value) : "—", c.market_value ? valueDet : "no market price yet", "var(--blue)"),
          tile("PRICE PAID", c.price_paid ? usd(c.price_paid) : "—", c.price_paid ? paidDet : "price paid not recorded"),
          tile("GAIN / LOSS", g ? g.text : "—", g ? g.pct : "needs both prices", g ? g.color : null)),
        h("h2", "Your Copies"),
        h("p.note", "One row per purchase, by finish and condition, with the price paid for each copy."),
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
        d.others && d.others.length ? [h("h2", "Other Printings You Own"), h("div.rows", d.others.map((o) => h("div.row", { onclick: () => openCard(o.key) },
          h("span.name", `${o.set_name} · ${sn(o)}`), h("span.where", copiesSummary(o)))))] : null))));
}

// showPrintings lists every printing of the card; picking one moves every copy there,
// keeping each purchase's finish, condition, quantity, price paid and date.
async function showPrintings(box, key, c) {
  fill(box, h("p.muted", "Finding every printing…"));
  try {
    const r = await api("GET", gameAPI(`/printings?key=${encodeURIComponent(key)}`));
    fill(box, h("h2", "Change printing"),
      h("p.note", "Pick the printing you own; every purchase moves with its condition, price paid and date."),
      h("div.list.results", r.printings.map((p) => h("div.card" + (p.id === r.current ? ".sel" : ""), {
        onclick: async () => {
          if (p.id === r.current) return;
          try {
            const res = await api("PUT", gameAPI(`/card/printing?key=${encodeURIComponent(key)}`), { id: p.id });
            if (ticks().has(key)) { ticks().delete(key); ticks().set(res.key, tickInfo(p)); }
            status(`Moved ${c.name} to ${sn(p)}${res.finishChanged ? ` (now ${res.finishes.map(fname).join(", ")}: the new printing doesn't come in every finish you had)` : ""}${res.merged ? ", merged with the copies you already had" : ""}`);
            await loadFilters();
            openCard(res.key);
          } catch (e) { status(e.message, true); }
        } },
        h("div.text", h("strong", `${p.set_name}${p.id === r.current ? "  (current)" : ""}`),
          h("span.meta", [sn(p), title(p.rarity), (p.released_at || "").slice(0, 4), (p.finishes || []).map(fname).join("/")].filter(Boolean).join(" · ")))))));
  } catch (e) { fill(box, h("p.error", e.message)); }
}

// --- Add and Scan ------------------------------------------------------------

// addForm shows a catalog card with the details of the copies to add.
function addForm(container, c, owned) {
  const qty = h("input", { type: "number", min: 1, step: 1, value: 1 });
  const finishes = c.finishes && c.finishes.length ? c.finishes : state.filters.finishes;
  const finish = h("select", finishes.map((x) => h("option", { value: x }, fname(x))));
  const cond = h("select", state.filters.conditions.map((x) => h("option", { value: x, selected: x === state.settings.default_condition }, x)));
  const price = h("input", { type: "number", min: 0, step: "0.01", placeholder: "0.00" });
  const date = h("input", { type: "date", value: state.settings.default_date === "today" ? today() : "" });
  const err = h("p.error");
  const add = async () => {
    const quantity = parseInt(qty.value, 10);
    if (!Number.isInteger(quantity) || quantity < 1) { err.textContent = "Quantity must be at least 1."; return; }
    try {
      const r = await api("POST", gameAPI("/cards"), { card: c, quantity, finish: finish.value, condition: cond.value,
        price_paid: price.value ? parseFloat(price.value) : 0, acquired_date: date.value });
      status(`Added ${quantity}× ${c.name} (${sn(c)})${r.alreadyOwned ? " to the copies you already had" : ""}`);
      await loadFilters();
      openCard(r.key);
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
          h("label", "Price paid (each)"), price, h("label", "Acquired"), date),
        err, h("button.primary", { type: "button", onclick: add }, "Add to collection")))));
}

// pickList lists catalog cards; clicking one shows it in `preview`.
function pickList(cards, owned, preview) {
  const items = cards.map((c, i) => h("div.card", { onclick: (e) => {
    e.currentTarget.parentElement.querySelectorAll(".sel").forEach((x) => x.classList.remove("sel"));
    e.currentTarget.classList.add("sel");
    addForm(preview, c, owned[i]);
  } },
  h("div.text", h("strong", c.name),
    h("span.meta", [sn(c), title(c.rarity), (c.released_at || "").slice(0, 4), anyPrice(c) ? usd(anyPrice(c)) : ""].filter(Boolean).join(" · "))),
  owned[i] ? h("span.owned", `own ${owned[i]}`) : null));
  return items;
}

// Add: search the game's catalog, pick the exact printing, set ownership details.
function viewAdd(params) {
  const pre = new URLSearchParams(params || "");
  const [textHint, setHint] = state.filters.searchHints;
  const text = h("input", { type: "search", placeholder: textHint, value: pre.get("text") || "", "aria-label": "Card name" });
  const set = h("input.set", { type: "search", placeholder: setHint, value: pre.get("set") || "", "aria-label": "Set" });
  const results = h("div.list.results", h("p.muted.empty", "Search by name, set code, or both, then pick a printing."));
  const preview = h("div");
  let next = "", cards = [], owned = [], total = 0;

  const render = () => {
    fill(results,
      h("p.muted.empty", cards.length ? (total > cards.length ? `Showing ${cards.length} of ${total} printings` : plural(total, "printing", "printings") + " found") : "No cards found."),
      pickList(cards, owned, preview),
      next ? h("button", { type: "button", style: "margin:8px 12px", onclick: () => run(true) }, "Load more results") : null);
  };
  const run = async (more) => {
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
      if (!more && cards.length) results.querySelector(".card").click();
    } catch (e) { status(e.message, true); }
  };
  const form = h("form.search", { onsubmit: (e) => { e.preventDefault(); run(false); } }, text, set, h("button.primary", { type: "submit" }, "Search"),
    gameInfo().Scan ? h("a.button", { href: "#/scan" }, "Scan a card") : null);
  fill($("#main"), h("h1", "Add Cards"), form, h("div.split", h("div.pane-list", results), preview));
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
      fill(result,
        h("p.scan-read", "Read: ", h("strong", r.read)),
        r.note ? h("p.note.scan-read", r.note) : null,
        r.candidates.length ? h("div.split",
          h("div.pane-list", h("div.list.results", pickList(r.candidates, r.owned, preview))), preview)
          : h("p.scan-read", h("a.button", { href: `#/add?${new URLSearchParams({ text: r.name || "" })}` }, "Search by hand")));
      if (r.candidates.length) result.querySelector(".results .card").click();
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
      g.Cards.length ? h("div.list", g.Cards.map((c) => h("div.card", { onclick: async () => { await switchTo(g.Game); go("collection", new URLSearchParams({ key: c.key })); } },
        h("div.text", h("span.name", c.name), h("span.meta", `${sn(c)} · ${copiesSummary(c)}`))))) : null,
      g.Total > g.Cards.length ? h("p.muted", `Showing the first ${g.Cards.length}; refine the search to see more.`) : null,
      h("p", h("button", { type: "button", onclick: async () => { await switchTo(g.Game); go("add", new URLSearchParams({ text: q })); } },
        `Search the ${g.Name} catalog for “${q}”`)),
    ]));
}

// --- Tools and Settings ------------------------------------------------------

// Import: a CSV export from another app, or a collection .json from this app.
function viewImport() {
  const file = h("input", { type: "file", accept: ".csv,.json,text/csv,application/json" });
  const out = h("div");
  const importBtn = h("button.primary", { type: "button", onclick: async () => {
    if (!file.files.length) { status("Choose a file first."); return; }
    importBtn.disabled = true;
    status(`Importing ${file.files[0].name}… matching cards can take a minute for large files.`);
    try {
      const r = await api("POST", gameAPI("/import"), file.files[0]);
      status(`Imported ${plural(r.cards, "card", "cards")} from ${r.format}`);
      fill(out, h("h2", "Import Complete"), h("pre.summary", r.summary));
      await loadFilters();
    } catch (e) { status(e.message, true); }
    importBtn.disabled = false;
  } }, "Import");
  fill($("#main"),
    h("h1", "Import"),
    h("p", "Import a CSV export from another collection app (EchoMTG, ManaBox, Moxfield, Deckbox, TCGplayer, Dragon Shield, Archidekt and others), or a collection .json file exported from this app. Cards you already own add to their quantities."),
    h("div.card-box", h("div.search", file, importBtn)), out);
}

// toolRow is one action on the Tools page with what it does.
const toolRow = (button, about) => h("div.tool", button, h("p.muted", about));

async function exportCollection(button) {
  if (!state.info.saveExports) { location.href = gameAPI("/export"); return; }
  button.disabled = true;
  try {
    const r = await api("POST", gameAPI("/export"));
    status(`Saved the collection to ${r.path}`);
  } catch (e) { status(e.message, true); }
  button.disabled = false;
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
      toolRow(h("button", { type: "button", onclick: () => { go("collection"); setTimeout(showBulkEdit, 50); } }, "Bulk edit…"),
        "Change the condition, finish, price paid or date of many cards at once, or delete them. Tick the cards in the collection list (filter it, then Select all shown); Bulk edit is also below the list."),
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
      row("Card order", menu("card_order", "card order"), "How cards are listed in the collection"),
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
    h("p.muted", "Your collections, price history and settings are kept in this folder. Back it up to keep them safe (Tools → Export collection saves a copy)."),
    h("p", h("code", d.dataDir || "(not saved)")),
    h("h2", "About"),
    h("p", `${state.info.name} ${state.info.version}`),
    h("p", h("button", { type: "button", onclick: async () => {
      if (!confirm("Put every setting back to how it was when the app was installed? Your collection isn't affected.")) return;
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
  const active = view === "import" ? "tools" : view === "scan" ? "add" : view;
  document.querySelectorAll("#nav a, .tabbar a").forEach((a) => a.classList.toggle("active", a.dataset.view === active));
  try {
    if (view === "collection") await viewCollection(qs || "");
    else if (view === "add") viewAdd(qs || "");
    else if (view === "scan") viewScan();
    else if (view === "search") await viewSearch(qs || "");
    else if (view === "import") viewImport();
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
    } else if (revision !== state.revision && !busy && !$("#dialog").open && !location.hash.startsWith("#/scan") && !location.hash.startsWith("#/add")) {
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
  setInterval(() => { if (!document.hidden) watchForChanges(); }, 15000);
  document.addEventListener("visibilitychange", () => { if (!document.hidden) watchForChanges(); });
  route();
}

init().catch((e) => fill($("#main"), h("p.error", `Could not start: ${e.message}`)));
