// MTG Manager web UI. Plain JavaScript, no build step. All text from the server is
// inserted with textContent (via h()), never as HTML, so card data can't inject markup.
"use strict";

const state = { info: null, game: null, filters: null, range: null, selectedKey: null };
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
const plural = (n, one, many) => `${n.toLocaleString()} ${n === 1 ? one : many}`;
function status(msg, isError) {
  const el = $("#status");
  el.textContent = msg || "";
  el.className = isError ? "error" : "";
}

async function api(method, path, body) {
  const opts = { method, headers: { "X-MTG-Client": "web" } };
  if (body instanceof FormData) opts.body = body;
  else if (body !== undefined) {
    opts.body = JSON.stringify(body);
    opts.headers["Content-Type"] = "application/json";
  }
  const res = await fetch(path, opts);
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(data.error || res.statusText);
  return data;
}
const gameAPI = (path) => `/api/games/${encodeURIComponent(state.game)}${path}`;

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

// lineChart draws series [{name, color, points:[{date,value}], step}] over [from, to].
function lineChart(series, from, to) {
  const W = 900, H = 280, L = 70, R = 12, T = 10, B = 26;
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
    h("div.label", { title: it.label }, it.label),
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

// --- Views -------------------------------------------------------------------

async function viewHome() {
  const range = state.range || state.info.defaultRange;
  const o = await api("GET", gameAPI(`/overview?range=${encodeURIComponent(range)}`));
  const g = gain(o.cost, o.market);
  const main = $("#main");
  const series = [
    { name: "Market value", color: "var(--blue)", points: o.chart.market },
    { name: "Price paid", color: "var(--amber)", points: o.chart.paid, step: true },
    { name: "At today's prices (estimate)", color: "var(--grey)", points: o.chart.estimate, step: true },
  ];
  const notes = [];
  if (o.canBackfill && !o.backfilled) notes.push(`Market value uses daily prices recorded each day the app runs. Load ${o.historyInfo} to fill in the history.`);
  else if (o.chart.historyStart) notes.push(`Market value: daily market prices since ${o.chart.historyStart}.`);
  notes.push("Price paid: running total by acquired date.");
  if (o.chart.estimate.length) notes.push("Before recorded prices begin, the grey line values the cards you owned at today's prices.");

  const backfill = o.canBackfill
    ? h("button", { type: "button", onclick: (e) => runJob(e.target, "/backfill", "Loading price history…", (r) => `Loaded price history for ${r.printings} printings`) },
        o.backfilled ? "Update price history" : "Load price history")
    : null;

  const top = h("div.rows", o.topCards.map((c, i) => h("div.row", { onclick: () => openCard(c.key) },
    h("span.name", `${i + 1}. ${c.name}`),
    h("span.where", `${sn(c)}${c.finish !== state.filters.finishes[0] ? " " + c.finish : ""}`),
    h("span.amt", c.quantity > 1 ? `${c.quantity} × ${usd(c.each)} = ${usd(c.value)}` : usd(c.value)))));

  const completion = o.bySet.map((x) => ({
    label: `${x.Code} · ${x.Name}`,
    value: x.Size > 0 ? x.Completion : 0,
    text: x.Size > 0 ? `${x.Owned} / ${x.Size}  (${Math.round(x.Completion * 100)}%)` : `${x.Owned} owned`,
  }));
  const byValue = [...o.bySet].filter((x) => x.Market > 0).sort((a, b) => b.Market - a.Market).slice(0, 10)
    .map((x) => ({ label: `${x.Code} · ${x.Name}`, value: x.Market, text: usd(x.Market) }));

  fill(main, 
    h("h1", o.name),
    h("div.tiles",
      tile("CARDS", o.copies.toLocaleString(), `${plural(o.entries, "entry", "entries")} · ${plural(o.sets, "set", "sets")}`),
      tile("PRICE PAID", usd(o.cost), o.costUnknown ? `${o.costUnknown} cards without a price paid` : "all cards have a price paid"),
      tile("MARKET VALUE", usd(o.market), `prices as of ${o.pricesAsOf || "never"}${o.marketUnknown ? ` · ${o.marketUnknown} unpriced` : ""}`, "var(--blue)"),
      tile("GAIN / LOSS", g.text, g.pct, g.color)),
    h("div.section-head", h("h2", "Value Over Time"),
      h("div.ranges", state.info.ranges.map((r) => h("button" + (r === range ? ".on" : ""), {
        type: "button", onclick: () => { state.range = r; store("range", r); route(); } }, r)))),
    legend(series),
    lineChart(series, o.chart.from, o.chart.to),
    h("p.note", notes.join(" ")),
    backfill,
    h("h2", "Set Completion"),
    completion.length ? bars(completion, 1, "var(--blue)") : h("p.muted", "No sets yet."),
    h("h2", "Value by Set"),
    byValue.length ? bars(byValue, byValue[0].value, "var(--amber)") : h("p.muted", "No market prices yet."),
    h("h2", "Most Valuable Cards"),
    o.topCards.length ? top : h("p.muted", "No market prices yet. Use Refresh prices."),
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
    button.disabled = false;
  }
}

// Collection: filters, sets with their cards, and the selected card's editor.
async function viewCollection(params) {
  const f = state.filters;
  const q = new URLSearchParams(params);
  const menu = (name, label, options, display) => h("select", {
    "aria-label": label, onchange: (e) => { const p = new URLSearchParams(location.hash.split("?")[1]); e.target.value ? p.set(name, e.target.value) : p.delete(name); p.delete("key"); go("collection", p); } },
    h("option", { value: "" }, label),
    options.map((o) => h("option", { value: o.value ?? o, selected: q.get(name) === (o.value ?? o) }, display ? display(o) : (o.label ?? o))));

  const search = h("input", { type: "search", placeholder: "Search name, set, or collector number", value: q.get("q") || "" });
  let timer;
  search.addEventListener("input", () => {
    clearTimeout(timer);
    timer = setTimeout(() => { const p = new URLSearchParams(location.hash.split("?")[1]); search.value ? p.set("q", search.value) : p.delete("q"); go("collection", p, true); }, 250);
  });

  const filters = h("div.filters", search, h("div.menus",
    menu("set", "Set", (f.sets || []).map((x) => ({ value: x.Code, label: x.Label }))),
    (f.filters || []).map((x) => menu("f." + x.Name, x.Name, x.Options)),
    menu("rarity", "Rarity", f.rarities, title),
    menu("finish", "Finish", f.finishes, title)));

  const apiQ = new URLSearchParams(q);
  apiQ.delete("key");
  const data = await api("GET", gameAPI(`/cards?${apiQ}`));
  const filtering = [...apiQ.keys()].length > 0;
  const list = h("div.list", data.sets.map((set) => h("details", { open: filtering || set.Cards.some((c) => c.key === q.get("key")) },
    h("summary", h("span.name", set.Name || set.Code), h("span.muted", set.Size ? `${set.Code} · ${set.Owned}/${set.Size}` : `${set.Code} · ${set.Owned}`)),
    set.Cards.map((c) => h("div.card" + (c.key === q.get("key") ? ".sel" : ""), { onclick: () => openCard(c.key) },
      h("span.num", c.collector_number), h("span.name", c.name),
      h("span.meta", [c.finish !== f.finishes[0] ? c.finish : null, c.condition !== "NM" ? c.condition : null, `×${c.quantity}`].filter(Boolean).join(" · ")))))));

  const summary = h("p.muted", filtering
    ? `${data.matched} of ${plural(data.total, "entry", "entries")} match · ${plural(data.copies, "card", "cards")}`
    : `${plural(data.total, "entry", "entries")} · ${plural(data.copies, "card", "cards")} · ${plural(data.sets.length, "set", "sets")}`);

  const detail = h("div.pane-detail");
  const split = h("div.split", h("div.pane-list", filters, summary, list), detail);
  fill($("#main"), split);
  if (q.get("key")) {
    split.classList.add("showing-detail");
    await renderCard(detail, q.get("key"));
  } else {
    detail.append(h("div.detail", h("p.muted", "Select a card to see and edit it.")));
  }
}

function openCard(key) {
  const p = new URLSearchParams(location.hash.split("?")[1]);
  if (!location.hash.startsWith("#/collection")) p.forEach((_, k) => p.delete(k));
  p.set("key", key);
  go("collection", p);
}

async function renderCard(container, key) {
  const d = await api("GET", gameAPI(`/card?key=${encodeURIComponent(key)}`));
  const c = d.card;
  const qty = h("input", { type: "number", min: 0, step: 1, value: c.quantity });
  const finishes = c.finishes && c.finishes.length ? c.finishes : [c.finish];
  const finish = h("select", finishes.map((x) => h("option", { value: x, selected: x === c.finish }, x)));
  const cond = h("select", state.filters.conditions.map((x) => h("option", { value: x, selected: x === c.condition }, x)));
  const price = h("input", { type: "number", min: 0, step: "0.01", placeholder: "0.00", value: c.purchase_price || "" });
  const date = h("input", { type: "date", value: c.acquired_date || "" });
  const err = h("p.error");
  const step = (n) => { qty.value = Math.max(0, (parseInt(qty.value, 10) || 0) + n); };

  const save = async () => {
    err.textContent = "";
    const body = { quantity: parseInt(qty.value, 10), finish: finish.value, condition: cond.value,
      purchase_price: price.value ? parseFloat(price.value) : 0, acquired_date: date.value };
    if (!Number.isInteger(body.quantity) || body.quantity < 0) { err.textContent = "Quantity must be a whole number (0 or more)."; return; }
    if (body.quantity === 0 && !confirm(`Remove all of ${c.name} from your collection?`)) return;
    try {
      const r = await api("PUT", gameAPI(`/card?key=${encodeURIComponent(key)}`), body);
      status(r.deleted ? `Deleted ${c.name}` : r.merged ? `Saved ${c.name} — merged into your existing copies` : `Saved ${c.name}`);
      if (r.deleted) { const p = new URLSearchParams(location.hash.split("?")[1]); p.delete("key"); go("collection", p); }
      else openCard(r.key);
    } catch (e) { err.textContent = e.message; }
  };
  const del = async () => {
    if (!confirm(`Remove all ${c.quantity} of ${c.name} (${sn(c)}, ${c.finish}, ${c.condition}) from your collection?`)) return;
    try {
      await api("DELETE", gameAPI(`/card?key=${encodeURIComponent(key)}`));
      status(`Deleted ${c.name}`);
      const p = new URLSearchParams(location.hash.split("?")[1]); p.delete("key"); go("collection", p);
    } catch (e) { err.textContent = e.message; }
  };

  const paidTotal = c.purchase_price * c.quantity;
  const g = c.value && c.purchase_price ? gain(paidTotal, c.value) : null;
  const order = (k) => { const i = state.filters.finishes.indexOf(k); return i < 0 ? 99 : i; };
  const marketText = Object.entries(c.market || {}).sort((a, b) => order(a[0]) - order(b[0]))
    .map(([k, v]) => `${usd(v)} ${k}`).join(" · ") + (c.prices_updated ? `  (as of ${c.prices_updated})` : "");
  const kv = [["Set", `${c.set_name} (${c.set})`], ["Collector number", c.collector_number], ["Rarity", title(c.rarity)],
    ...(d.details || []).map((r) => [r.Label, r.Value]),
    c.lang && c.lang !== "en" ? ["Language", c.lang.toUpperCase()] : null,
    c.released_at ? ["Released", c.released_at] : null,
    ["Printed in", (c.finishes || []).join(", ")], ["Market prices", marketText || "not available yet"]].filter(Boolean);

  fill(container, h("div.detail",
    h("button.back", { type: "button", onclick: () => { const p = new URLSearchParams(location.hash.split("?")[1]); p.delete("key"); go("collection", p); } }, "← Back"),
    h("h1", c.name),
    h("p.muted", [c.set_name, sn(c), title(c.rarity)].filter(Boolean).join(" · ")),
    h("div.detail-body",
      h("div", c.image_uri ? h("img.cardimg", { src: c.image_uri, alt: c.name, loading: "lazy" }) : h("div.cardimg"),
        d.link.url ? h("p", { style: "text-align:center" }, h("a", { href: d.link.url, target: "_blank", rel: "noopener noreferrer" }, d.link.label)) : null),
      h("div",
        h("div.tiles", { style: "grid-template-columns:repeat(auto-fit,minmax(140px,1fr))" },
          tile("MARKET VALUE", c.value ? usd(c.value) : "—", c.each ? `${usd(c.each)} each` : `no market price for ${c.finish}`, "var(--blue)"),
          tile("PRICE PAID", c.purchase_price ? usd(paidTotal) : "—", c.purchase_price ? `${usd(c.purchase_price)} each` : "price paid not recorded"),
          tile("GAIN / LOSS", g ? g.text : "—", g ? g.pct : "needs both prices", g ? g.color : null)),
        h("h2", "Your Copies"),
        h("div.form",
          h("label", "Quantity"), h("div.qty", h("button", { type: "button", onclick: () => step(-1), "aria-label": "Fewer" }, "−"), qty, h("button", { type: "button", onclick: () => step(1), "aria-label": "More" }, "+")),
          h("label", "Finish"), finish, h("label", "Condition"), cond,
          h("label", "Price paid (each)"), price, h("label", "Acquired"), date),
        err,
        h("div.buttons", h("button.danger", { type: "button", onclick: del }, "Delete"),
          h("div.right", h("button", { type: "button", onclick: () => openCard(key) }, "Revert"), h("button.primary", { type: "button", onclick: save }, "Save changes"))),
        h("h2", "Card"),
        c.type_line ? h("p", h("strong", c.type_line)) : null,
        c.oracle_text ? h("p.rules", c.oracle_text) : null,
        h("dl.kv", kv.flatMap(([k, v]) => [h("dt", k), h("dd", v)])),
        d.others && d.others.length ? [h("h2", "Other Copies You Own"), h("div.rows", d.others.map((o) => h("div.row", { onclick: () => openCard(o.key) },
          h("span.name", `${o.set_name} · ${sn(o)}`), h("span.where", `${o.finish} · ${o.condition} · ×${o.quantity}`))))] : null))));
}

// Add: search the game's catalog, pick the exact printing, set ownership details.
function viewAdd() {
  const [textHint, setHint] = state.filters.searchHints;
  const text = h("input", { type: "search", placeholder: textHint, autofocus: true });
  const set = h("input.set", { type: "search", placeholder: setHint });
  const results = h("div.list.results");
  const preview = h("div.detail", h("p.muted", "Search by name, set code, or both, then pick a printing."));
  let next = "";
  let items = [];

  const showPick = (c) => {
    const qty = h("input", { type: "number", min: 1, step: 1, value: 1 });
    const finishes = c.finishes && c.finishes.length ? c.finishes : state.filters.finishes;
    const finish = h("select", finishes.map((x) => h("option", { value: x }, x)));
    const cond = h("select", state.filters.conditions.map((x) => h("option", { value: x }, x)));
    const price = h("input", { type: "number", min: 0, step: "0.01", placeholder: "0.00" });
    const date = h("input", { type: "date", value: new Date().toISOString().slice(0, 10) });
    const err = h("p.error");
    const add = async () => {
      const quantity = parseInt(qty.value, 10);
      if (!Number.isInteger(quantity) || quantity < 1) { err.textContent = "Quantity must be at least 1."; return; }
      try {
        const r = await api("POST", gameAPI("/cards"), { card: c, quantity, finish: finish.value, condition: cond.value,
          purchase_price: price.value ? parseFloat(price.value) : 0, acquired_date: date.value });
        status(`${r.merged ? "Added to existing entry:" : "Added"} ${quantity}× ${c.name} (${sn(c)})`);
        await loadFilters();
        openCard(r.key);
      } catch (e) { err.textContent = e.message; }
    };
    fill(preview, 
      h("h1", c.name), h("p.muted", `${c.set_name} — ${sn(c)}`),
      h("div.detail-body",
        c.image_uri ? h("img.cardimg", { src: c.image_uri, alt: c.name }) : h("div.cardimg"),
        h("div", c.type_line ? h("p", h("strong", c.type_line)) : null, c.oracle_text ? h("p.rules", c.oracle_text) : null,
          h("div.form", h("label", "Quantity"), qty, h("label", "Finish"), finish, h("label", "Condition"), cond,
            h("label", "Price paid (each)"), price, h("label", "Acquired"), date),
          err, h("button.primary", { type: "button", onclick: add }, "Add to collection"))));
  };

  const render = (total) => {
    fill(results, 
      h("p.muted", { style: "padding:8px 12px;margin:0" }, items.length ? (total > items.length ? `Showing ${items.length} of ${total} printings` : plural(total, "printing", "printings") + " found") : "No cards found."),
      items.map((c) => h("div.card", { onclick: (e) => { results.querySelectorAll(".sel").forEach((x) => x.classList.remove("sel")); e.currentTarget.classList.add("sel"); showPick(c); } },
        h("strong", c.name),
        h("span.meta", [sn(c), title(c.rarity), (c.released_at || "").slice(0, 4), c.each ? usd(c.each) : Object.values(c.market || {})[0] ? usd(Object.values(c.market)[0]) : ""].filter(Boolean).join(" · ")))),
      next ? h("button", { type: "button", style: "margin:8px 12px", onclick: () => run(true) }, "Load more results") : null);
  };

  const run = async (more) => {
    if (!more) {
      if (!text.value.trim() && !set.value.trim()) { status("Enter a card name, a set code, or both."); return; }
      items = []; next = "";
    }
    status("Searching…");
    try {
      const p = new URLSearchParams({ text: text.value, set: set.value });
      if (more && next) p.set("next", next);
      const r = await api("GET", gameAPI(`/search?${p}`));
      items = items.concat(r.cards);
      next = r.next;
      render(r.total);
      status("");
      if (!more && items.length) { results.querySelector(".card").click(); }
    } catch (e) { status(e.message, true); }
  };
  const form = h("form.search", { onsubmit: (e) => { e.preventDefault(); run(false); } }, text, set, h("button.primary", { type: "submit" }, "Search"));
  fill($("#main"), h("h1", "Add Card"), form, h("div.split", h("div.pane-list", results), h("div", preview)));
  text.focus();
}

// Import: a CSV export from another app, or a collection .json from this app.
function viewImport() {
  const file = h("input", { type: "file", accept: ".csv,.json,text/csv,application/json" });
  const out = h("div");
  const importBtn = h("button.primary", { type: "button", onclick: async () => {
    if (!file.files.length) { status("Choose a file first."); return; }
    const fd = new FormData();
    fd.append("file", file.files[0]);
    importBtn.disabled = true;
    status(`Importing ${file.files[0].name}… matching cards can take a minute for large files.`);
    try {
      const r = await api("POST", gameAPI("/import"), fd);
      status(`Imported ${r.copies} cards from ${r.format}`);
      fill(out, h("h2", "Import Complete"), h("pre.summary", r.summary));
      await loadFilters();
    } catch (e) { status(e.message, true); }
    importBtn.disabled = false;
  } }, "Import");
  fill($("#main"), 
    h("h1", "Import"),
    h("p", "Import a CSV export from another collection app (EchoMTG, ManaBox, Moxfield, Deckbox, TCGplayer, Dragon Shield, Archidekt and others), or a collection .json file exported from this app. Cards you already own add to their quantities."),
    h("p", file, " ", importBtn), out);
}

// --- Routing -----------------------------------------------------------------

function go(view, params, replace) {
  const qs = params && [...params.keys()].length ? `?${params}` : "";
  const hash = `#/${view}${qs}`;
  if (replace) { history.replaceState(null, "", hash); route(); } else location.hash = hash;
}

async function loadFilters() {
  state.filters = await api("GET", gameAPI("/filters"));
}

async function route() {
  const [path, qs] = location.hash.replace(/^#\/?/, "").split("?");
  const view = path || "home";
  document.querySelectorAll("#nav a").forEach((a) => a.classList.toggle("active", a.dataset.view === view));
  try {
    if (view === "collection") await viewCollection(qs || "");
    else if (view === "add") viewAdd();
    else if (view === "import") viewImport();
    else await viewHome();
  } catch (e) {
    fill($("#main"), h("p.error", e.message));
  }
}

async function init() {
  state.info = await api("GET", "/api/info");
  state.range = store("range") || state.info.defaultRange;
  const saved = store("game");
  state.game = state.info.games.some((g) => g.ID === saved) ? saved : state.info.games[0].ID;
  const sel = $("#game");
  fill(sel, ...state.info.games.map((g) => h("option", { value: g.ID, selected: g.ID === state.game }, g.Name)));
  sel.hidden = state.info.games.length < 2;
  sel.addEventListener("change", async () => { state.game = sel.value; store("game", sel.value); await loadFilters(); go("home"); });
  $("#refresh").addEventListener("click", (e) => runJob(e.target, "/refresh", "Refreshing prices…", (r) => `Prices updated for ${r.updated} entries`));
  $("#export").addEventListener("click", (e) => { e.preventDefault(); location.href = gameAPI("/export"); });
  document.title = state.info.name;
  await loadFilters();
  window.addEventListener("hashchange", route);
  route();
}

init().catch((e) => fill($("#main"), h("p.error", `Could not start: ${e.message}`)));
