// Pieces several pages share about cards: copy summaries, rows and thumbnails,
// opening a card, ticked cards, tags (chips and the tag editor), and the dialogs for
// adding a card, adding it to a list and watching its price.

import { api, gameAPI, loadFilters } from "./api.js";
import { $, fill, h, status, store } from "./dom.js";
import { anyPrice, englishPrice, finishRank, fname, plural, sn, title, today, usd } from "./format.js";
import { go, hashParams, route } from "./nav.js";
import { state } from "./state.js";

// copiesSummary describes a card's copies compactly: "×3" when they're all in the usual
// finish and near mint, otherwise per finish, e.g. "Normal ×2 · Foil ×1 (1 LP)".
export function copiesSummary(c) {
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

export function cardRows(cards, amount) {
  return h("div.rows", cards.map((c, i) => h("button.row", { type: "button", onclick: () => openCard(c.key) },
    h("span.name", `${i + 1}. ${c.name}`),
    h("span.where", [sn(c), copiesSummary(c), englishPrice(c)].filter(Boolean).join(" · ")),
    h("span.amt", usd(amount(c))))));
}

// ticks is the ticked cards of the current game.
export function ticks() {
  if (!state.ticked.has(state.game)) state.ticked.set(state.game, new Map());
  return state.ticked.get(state.game);
}

export const tickInfo = (c) => ({ name: c.name, where: sn(c), tags: cardTagList(c) });

// The core's limits on tags (crates/core/src/collection/tags.rs).
const TAG_MAX_LEN = 40;

const TAGS_MAX = 20;

// tagProblem says why `raw` can't be a tag, or "" if it can (as clean_tag decides).
export function tagProblem(raw) {
  const clean = tidyTag(raw);
  if (/[\u0000-\u001f\u007f-\u009f]/.test(raw)) return "A tag can't contain control characters.";
  if (!clean) return "A tag can't be empty.";
  if ([...clean].length > TAG_MAX_LEN) return `A tag can be at most ${TAG_MAX_LEN} characters.`;
  if (/[,;]/.test(clean)) return "A tag can't contain a comma or a semicolon.";
  return "";
}

export const tidyTag = (raw) => raw.trim().split(/\s+/).filter(Boolean).join(" ");

export const sameTag = (a, b) => a.toLowerCase() === b.toLowerCase();

export const sortTags = (tags) => tags.sort((a, b) => a.toLowerCase().localeCompare(b.toLowerCase()));
// cardTagList is a card's tags: the union across its purchases.

export const cardTagList = (c) => sortTags([...new Map((c.purchases || []).flatMap((p) => p.tags || []).map((t) => [t.toLowerCase(), t])).values()]);

// tagHue gives a tag a hue from its name, so the same tag always looks the same.
function tagHue(name) {
  let hash = 0;
  for (const ch of name.toLowerCase()) hash = (hash * 31 + ch.codePointAt(0)) >>> 0;
  return hash % 360;
}

export const tagStyle = (name) => `--tag-hue:${tagHue(name)}`;

// tagChip is a tag that does something when clicked (a button).
export const tagChip = (name, attrs, ...children) => h("button.tag-chip", { type: "button", style: tagStyle(name), ...attrs }, name, ...children);

// tagEditor edits a list of tags: chips with a remove button, then a text box where
// Enter, a comma or a semicolon adds what's typed and Backspace removes the last chip.
// `suggest` lists tags to offer (default: every tag in use). It returns { el, value,
// focus }; value() adds any text still typed and throws if it can't be a tag.
let tagEditors = 0;

export function tagEditor(initial, { suggest, label = "Tags", onChange } = {}) {
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

// smallImage is Scryfall's small image for a card's normal one.
const smallImage = (uri) => uri.replace("/normal/", "/small/");

// thumbnail is a card's picture in a list row (small) or the grid (normal), when images
// are on. The grid keeps a placeholder without one.
export function thumbnail(c, grid) {
  if (!state.settings.show_images || !c.image_uri) return grid ? h("span.thumb.none", c.name) : null;
  const src = grid ? c.image_uri : smallImage(c.image_uri);
  return h("img.thumb", { src, alt: "", loading: "lazy", decoding: "async",
    onerror: (e) => { if (e.target.src !== c.image_uri) e.target.src = c.image_uri; } });
}

// addToList puts catalog card `c` on a list (or a new one), in a dialog of its own so it
// can open over another.
export async function addToList(c, quantity) {
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

export const ALERT_NOTE = "Prices are checked when the app refreshes them (each day, or as Settings says), so alerts appear while the app is open or the next time you open it. There is no email or text message.";

// showWatch starts watching catalog card `c` (in `finish`, if given), or changes its
// alert when `w` (a watch) is given.
export function showWatch(c, finish, w) {
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

// addInDialog shows a catalog card's add form in the dialog, leaving the page as it is.
export function addInDialog(c, added) {
  const dlg = $("#dialog");
  const body = h("div.body");
  fill(dlg, h("div.dlg", h("header", h("h2", "Add a card")), body,
    h("footer", h("button", { type: "button", onclick: () => dlg.close() }, "Close"))));
  addForm(body, c, 0, () => { dlg.close(); if (added) added(); });
  dlg.showModal();
}

// cardImage is the card's picture, or a placeholder when images are off in Settings.
export function cardImage(c, lazy) {
  if (!state.settings.show_images) return h("div.cardimg.off", "Card images are off (Settings)");
  return c.image_uri ? h("img.cardimg", { src: c.image_uri, alt: c.name, loading: lazy ? "lazy" : null }) : h("div.cardimg");
}

export function openCard(key) {
  const p = location.hash.startsWith("#/collection") ? hashParams() : new URLSearchParams();
  p.set("key", key);
  // On a narrow screen the card replaces the list: Back returns to this spot.
  const narrow = matchMedia("(max-width: 1000px)").matches;
  if (narrow && location.hash.startsWith("#/collection")) state.listWindowY = window.scrollY;
  go("collection", p);
  if (narrow) window.scrollTo(0, 0);
}

export function closeCard() {
  const p = hashParams();
  p.delete("key");
  go("collection", p);
}

// addDefaults are the finish, condition, price and date of the last card added, so
// adding many cards from one purchase needs them typed once.
export function addDefaults() {
  return state.addDefaults || { finish: "", condition: state.settings.default_condition, price: "", date: state.settings.default_date === "today" ? today() : "" };
}

// addCard adds copies of a catalog card and remembers the details for the next one.
export async function addCard(c, d) {
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
export function pickList(cards, owned, preview, onAdded, lot) {
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
