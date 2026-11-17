// The Collection page: unfinished lots' banners, the filters, chips and sort, every
// matching card as a line item with its copies (pages/copies.js), a lot's panel when
// the list is filtered to one, the set page, and exporting what is listed.

import { api, exportCollection, gameAPI, loadLots } from "../api.js";
import { addInDialog, addToList, cardRows, sameTag, tagChip, tickInfo, ticks } from "../cards.js";
import { gain, tile } from "../charts.js";
import { $, askDialog, fill, h, status } from "../dom.js";
import { anyPrice, choiceNames, compareNumber, fname, languageName, plural, title, usd } from "../format.js";
import { go, hashParams, route } from "../nav.js";
import { state } from "../state.js";
import { showBulkEdit } from "./bulk.js";
import { openAdd } from "./add.js";
import { renderCard } from "./card.js";
import { cardLine } from "./copies.js";
import { lotPanel, lotProgress } from "./lots.js";

// The collection list's sort keys (as GET /cards takes them) and labels.
const SORTS = [["name", "Name"], ["set", "Set"], ["number", "Number"], ["price", "Price each"], ["value", "Value"],
  ["gain", "Gain / loss"], ["paid", "Price paid"], ["quantity", "Quantity"], ["acquired", "Acquired"], ["released", "Released"], ["rarity", "Rarity"],
  ["tag", "Tag"]];
// Keys that start ascending when chosen; the rest start with the most first.

const ASCENDING_FIRST = ["name", "number", "tag"];
// How many cards the list draws at a time.
const PAGE = 60;
// Collection URL parameters that aren't filters.

const LIST_KEYS = ["key", "sort", "dir"];
// The filters in the More filters panel.

const MORE_KEYS = ["condition", "language", "card_type", "price_min", "price_max", "acquired_from", "acquired_to", "min_copies", "no_price_paid", "unpriced",
  "tags", "tags_mode", "tags_not", "untagged"];

const FLAGS = [["no_price_paid", "No price paid"], ["unpriced", "No market price"], ["untagged", "Untagged"]];
// The tags in a filter parameter ("binder A,elves deck").

const paramTags = (q, name) => (q.get(name) || "").split(",").map((t) => t.trim()).filter(Boolean);

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
      (f.languages || []).length ? menu("language", "Language", f.languages.map((l) => ({ value: l.code, label: l.name }))) : null),
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
    case "lot": return v === "none" ? "Bought in no lot" : `Bought in: ${lots.find((l) => l.id === v)?.name || v}`;
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
export async function viewCollection(params) {
  const f = state.filters;
  const q = new URLSearchParams(params);
  const nav = (mutate, replace) => { const p = hashParams(); mutate(p); go("collection", p, replace); };
  // A card chosen: its page, in place of the list.
  if (q.get("key")) {
    const detail = h("div.pane-detail.full");
    fill($("#main"), detail);
    await renderCard(detail, q.get("key"));
    return;
  }
  const menu = (name, label, options, display) => h("select" + (q.get(name) ? ".on" : ""), {
    "aria-label": label, onchange: (e) => nav((p) => { e.target.value ? p.set(name, e.target.value) : p.delete(name); p.delete("key"); }) },
    h("option", { value: "" }, label),
    options.map((o) => h("option", { value: o.value ?? o, selected: q.get(name) === (o.value ?? o) }, display ? display(o) : (o.label ?? o))));

  const search = h("input", { type: "search", placeholder: "Filter this list by name, set, number or tag", value: q.get("q") || "", "aria-label": "Filter the collection" });
  let timer;
  search.addEventListener("input", () => {
    clearTimeout(timer);
    timer = setTimeout(() => nav((p) => { search.value ? p.set("q", search.value) : p.delete("q"); }, true), 250);
  });
  const setMenu = menu("set", "Set", (f.sets || []).map((x) => ({ value: x.Code, label: x.Label })));
  setMenu.classList.add("set-menu"); // set names are long: the menu gets its own line

  const apiQ = new URLSearchParams(q);
  for (const k of LIST_KEYS) if (k !== "sort" && k !== "dir") apiQ.delete(k);
  const [data, lots, tagTotals] = await Promise.all([api("GET", gameAPI(`/cards?${apiQ}`)), loadLots(),
    (state.filters.tags || []).length ? api("GET", gameAPI("/tags")) : { tags: [] }]);
  const filtering = [...apiQ.keys()].some((k) => !LIST_KEYS.includes(k));
  // The game's first filter (Magic's Color) stays in view; the rest go in More filters.
  const [firstFilter, ...otherFilters] = f.filters || [];
  const lotMenu = lots.length ? menu("lot", "Bought in", [{ value: "none", label: "No lot" }, ...lots.map((l) => ({ value: l.id, label: `📦 ${l.name}` }))]) : null;
  const filters = h("div.filters", search, h("div.menus",
    setMenu,
    firstFilter ? menu("f." + firstFilter.Name, firstFilter.Name, firstFilter.Options) : null,
    menu("rarity", "Rarity", f.rarities, title),
    menu("finish", "Finish", f.finishes, fname),
    lotMenu));
  filters.append(...[moreFilters(q, nav, menu, otherFilters, lots, tagTotals.tags), filterChips(q, nav, lots)].filter(Boolean));
  const ticked = ticks();
  if (!filtering) { // every card is listed: drop ticks for cards no longer owned
    const keys = new Set(data.cards.map((c) => c.key));
    for (const k of [...ticked.keys()]) if (!keys.has(k)) ticked.delete(k);
  }

  // Sorting, kept in the URL with the filters.
  const sortMenu = h("select", { "aria-label": "Sort by", onchange: (e) => nav((p) => {
    p.set("sort", e.target.value);
    p.set("dir", ASCENDING_FIRST.includes(e.target.value) ? "asc" : "desc");
  }, true) }, SORTS.map(([v, label]) => h("option", { value: v, selected: data.sort === v }, `Sort: ${label}`)));
  const desc = data.dir === "desc";
  const dirButton = h("button.dir", { type: "button", "aria-label": desc ? "Descending; switch to ascending" : "Ascending; switch to descending",
    title: desc ? "Descending" : "Ascending",
    onclick: () => nav((p) => { p.set("sort", data.sort); p.set("dir", desc ? "asc" : "desc"); }, true) }, desc ? "↓" : "↑");
  const listbar = h("div.listbar", sortMenu, dirButton);

  // The line items, a page at a time. A change saved on a line redraws the page
  // (keeping the scroll); a split not saved yet just redraws the lines.
  let shown = Math.max(PAGE, state.linesShown && state.listKey === `${apiQ}` ? state.linesShown : 0);
  const list = h("div.lines");
  const more = h("button", { type: "button", onclick: () => { shown += PAGE; drawLines(); } }, "Show more");
  const ctx = {
    lots,
    onTick: () => updateBar(),
    redraw: async (refetch = true) => {
      if (!refetch) { drawLines(); return; }
      const y = window.scrollY;
      state.linesShown = shown;
      await loadLots();
      await route();
      window.scrollTo(0, y);
    },
  };
  function drawLines() {
    fill(list, data.cards.length ? data.cards.slice(0, shown).map((c) => cardLine(c, ctx)) : h("p.muted.empty", "No cards match."),
      data.cards.length > shown ? h("p.more", more, h("span.muted", ` ${shown} of ${data.cards.length}`)) : null);
  }
  drawLines();
  state.listKey = `${apiQ}`;

  const counts = `${plural(data.uniqueCards, "unique card", "unique cards")} · ${plural(data.cardCount, "card", "cards")} · ${plural(data.sets, "set", "sets")}`;
  const money = `${usd(data.marketValue)} value · ${usd(data.pricePaid)} paid`;
  const clear = filtering ? h("button.link", { type: "button", onclick: () => {
    const keep = new URLSearchParams([...q.entries()].filter(([k]) => LIST_KEYS.includes(k) && k !== "key"));
    go("collection", keep);
  } }, "Clear filters") : null;
  const exportButton = data.cards.length ? h("button.link", { type: "button", onclick: () => exportListed(q, data.cards.length) }, "Export…") : null;

  // The selection bar: how many are ticked, and the bulk edit actions. It sits above
  // the list and stays in view under the search bar as the list scrolls.
  const count = h("span.count");
  const selectAll = h("button", { type: "button", onclick: () => { data.cards.forEach((c) => ticked.set(c.key, tickInfo(c))); drawLines(); updateBar(); } }, "Select all shown");
  const clearTicks = h("button", { type: "button", onclick: () => { ticked.clear(); drawLines(); updateBar(); } }, "Clear");
  const bulk = h("button.primary", { type: "button", onclick: () => showBulkEdit() }, "Bulk edit…");
  const bar = h("div.selbar", count, selectAll, clearTicks, bulk);
  function updateBar() {
    const listed = new Set(data.cards.map((c) => c.key));
    const hidden = [...ticked.keys()].filter((k) => !listed.has(k)).length;
    count.textContent = !ticked.size ? "Tick cards to bulk edit them" :
      `${plural(ticked.size, "card", "cards")} ticked${hidden ? ` (${hidden} hidden by filters)` : ""}`;
    bar.classList.toggle("ticked", ticked.size > 0);
    selectAll.disabled = !data.cards.length;
    clearTicks.disabled = bulk.disabled = !ticked.size;
  }
  updateBar();

  // Unfinished lots: keep entering their cards from here.
  const banners = lots.filter((l) => l.open && l.id !== q.get("lot")).map((l) => h("div.card-box.notice.lot-banner",
    h("div.grow", h("strong", "📦 ", l.name), h("span.muted", ` — ${lotProgress(l)} · ${usd(l.total_paid)} paid`),
      l.expected ? h("div.bar", h("i", { style: `width:${Math.min(100, (l.entered / l.expected) * 100)}%` })) : null),
    h("button.primary", { type: "button", onclick: () => openAdd({ lot: l.id }) }, "Keep adding"),
    h("button", { type: "button", onclick: () => nav((p) => { p.set("lot", l.id); }) }, "Show its cards")));
  const lot = lots.find((l) => l.id === q.get("lot"));
  const setBox = h("div");
  fill($("#main"),
    h("datalist", { id: "tag-names" }, (f.tags || []).map((t) => h("option", { value: t }))),
    banners, lot ? lotPanel(lot, (id) => openAdd({ lot: id })) : null, setBox,
    h("div.collection", filters,
      h("p.counts.muted", h("span", filtering ? `Matching: ${counts}` : counts, h("span.money-line", ` · ${money}`)), h("span.count-actions", clear, exportButton)),
      listbar, bar, list));
  if (q.get("set")) await renderSet(setBox, q.get("set"));
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

// exportListed offers the collection list's cards as CSV or a valuation report.
async function exportListed(params, count) {
  const format = await askDialog("Export these cards", [
    `The ${plural(count, "card", "cards")} the list shows, in its order.`,
    "CSV opens in spreadsheets and other collection apps, and Import here reads it back. The valuation report is a page to print or save as PDF, with every card's value by set.",
  ], [{ label: "Valuation report", value: "report" }, { label: "CSV", value: "csv", primary: true }]);
  if (format) exportCollection(null, format, params);
}
