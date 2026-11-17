// The collection's line items: one line per card, its copies listed under it, and the
// actions on each row of copies (quantity, finish, condition, tags, lot, sell, split,
// delete). Each change is saved at once; the collection page draws the list.

import { api, gameAPI } from "../api.js";
import { openCard, sameTag, tagProblem, tagStyle, thumbnail, tickInfo, ticks, tidyTag } from "../cards.js";
import { ask, h, status } from "../dom.js";
import { fname, plural, sn, title, today, usd, usd4 } from "../format.js";
import { go, hashParams } from "../nav.js";
import { state } from "../state.js";

// filterBy adds a filter to the collection's current ones (a tag joins the tags
// chosen; a lot replaces the lot), so clicking one tag and then another shows the
// cards with both.
function filterBy(e, name, value) {
  e.preventDefault();
  const p = location.hash.startsWith("#/collection") ? hashParams() : new URLSearchParams();
  p.delete("key");
  if (name === "tags") {
    const tags = (p.get("tags") || "").split(",").map((t) => t.trim()).filter(Boolean);
    if (tags.some((t) => sameTag(t, value))) return;
    p.set("tags", [...tags, value].join(","));
    p.set("tags_not", (p.get("tags_not") || "").split(",").filter((t) => t.trim() && !sameTag(t.trim(), value)).join(","));
    if (!p.get("tags_not")) p.delete("tags_not");
  } else {
    p.set(name, value);
  }
  go("collection", p);
}

// Split groups not saved yet, per card key: [{ id, sig, row, qty }]. A split copy is the
// same as its group until something about it changes, and the app keeps identical
// copies together, so a new group lives here until it's changed (then it's saved) and
// is gone after a reload.
const pending = new Map();
// The group each group's last split made (by row ID), so more splits add to it.
const splitInto = new Map();
// The order rows were shown in, per card key (by signature): new groups go at the end
// and changed rows keep their place, for as long as the app is open.
const order = new Map();
let nextId = 1;

// What makes copies one group: everything but the quantity.
const sig = (p) => [p.finish, p.condition, p.price_paid || 0, p.acquired_date || "", p.lot || "", (p.tags || []).map((t) => t.toLowerCase()).sort().join(";")].join("|");

// The rows a card shows: its saved groups (less the copies split off them, not yet
// saved) in the order they were first shown, then the unsaved split groups.
function rowsOf(c) {
  const pend = (pending.get(c.key) || []).filter((g) => g.qty > 0);
  pending.set(c.key, pend);
  const known = order.get(c.key) || [];
  const saved = c.purchases.map((p) => {
    const s = sig(p);
    const taken = pend.filter((g) => g.sig === s).reduce((n, g) => n + g.qty, 0);
    return { ...p, sig: s, id: `s:${s}`, saved: p, qty: p.quantity - taken };
  });
  const seen = known.filter((s) => saved.some((r) => r.sig === s));
  for (const r of saved) if (!seen.includes(r.sig)) seen.push(r.sig);
  order.set(c.key, seen);
  saved.sort((a, b) => seen.indexOf(a.sig) - seen.indexOf(b.sig));
  return [...saved.filter((r) => r.qty > 0), ...pend.map((g) => ({ ...g.row, sig: g.sig, id: `p:${g.id}`, group: g, qty: g.qty }))];
}

// The purchases to save for card `c`: every row as shown, with `changed` (a row's ID →
// its new fields) applied. Unsaved split groups that didn't change go back into their
// group on the server, and stay split here.
function purchasesOf(c, changed) {
  return rowsOf(c).map((r) => {
    const p = { finish: r.finish, condition: r.condition, quantity: r.qty, price_paid: r.price_paid || 0, acquired_date: r.acquired_date || "",
      lot: r.lot || "", tags: r.tags || [], ...(changed[r.id] || {}) };
    return p;
  }).filter((p) => p.quantity > 0);
}

// cardLine is one card's line item and its rows of copies. `redraw` redraws the list
// after a change is saved; `lots` names the lots. `shows` (from the list's lot and
// tag filters) says which groups of copies to show and count; the others are kept,
// and saved untouched, but hidden.
export function cardLine(c, { redraw, lots, onTick, shows = null }) {
  const ticked = ticks();
  const box = h("input", { type: "checkbox", checked: ticked.has(c.key), "aria-label": `Tick ${c.name} for bulk edit`,
    onchange: (e) => { e.target.checked ? ticked.set(c.key, tickInfo(c)) : ticked.delete(c.key); onTick(); } });
  const lotName = (id) => (lots.find((l) => l.id === id) || {}).name || "a lot";
  const inLots = [...new Set(c.purchases.map((p) => p.lot).filter(Boolean))];
  const rows = rowsOf(c);
  const shown = shows ? rows.filter(shows) : rows;
  const copies = shown.reduce((n, r) => n + r.qty, 0);
  const hidden = rows.reduce((n, r) => n + r.qty, 0) - copies;
  const worth = shown.reduce((n, r) => n + (r.market || 0) * r.qty, 0);
  const priced = shown.some((r) => r.market > 0);

  // save sends the card's rows with `changed` applied, then forgets the unsaved group
  // that was saved (`savedGroup`), keeping its place for its new signature.
  const save = async (changed, savedGroup, message) => {
    const purchases = purchasesOf(c, changed);
    try {
      if (!purchases.length) {
        if (!(await ask(`Remove ${c.name} from your collection? It has no copies left.`, "Remove"))) return;
        await api("DELETE", gameAPI(`/card?key=${encodeURIComponent(c.key)}`));
      } else {
        await api("PUT", gameAPI(`/card?key=${encodeURIComponent(c.key)}`), { purchases });
      }
      const seen = order.get(c.key) || [];
      for (const [id, fields] of Object.entries(changed)) {
        const row = rowsOf(c).find((r) => r.id === id);
        if (!row) continue;
        const now = sig({ ...row, ...fields });
        const at = seen.indexOf(row.sig);
        if (id.startsWith("s:") && at >= 0) seen[at] = now; else if (!seen.includes(now)) seen.push(now);
      }
      if (savedGroup) pending.set(c.key, (pending.get(c.key) || []).filter((g) => g !== savedGroup));
      if (message) status(message);
      await redraw();
    } catch (e) { status(e.message, true); }
  };
  const change = (r, fields, message) => save({ [r.id]: fields }, r.group, message);

  const split = (r) => {
    if (r.qty < 2) return;
    const list = pending.get(c.key) || [];
    const child = list.find((g) => g.id === splitInto.get(r.id) && g.qty > 0);
    if (child) {
      child.qty++;
    } else {
      const fresh = { id: nextId++, sig: r.sig, row: { finish: r.finish, condition: r.condition, price_paid: r.price_paid, acquired_date: r.acquired_date, lot: r.lot, tags: [...(r.tags || [])], market: r.market }, qty: 1 };
      list.push(fresh);
      pending.set(c.key, list);
      splitInto.set(r.id, fresh.id);
    }
    if (r.group) r.group.qty--;
    redraw(false);
  };

  const rowEl = (r) => {
    const el = h("div.copy");
    const finishes = c.finishes && c.finishes.length ? c.finishes : [r.finish];
    const stepper = h("span.qty",
      h("button", { type: "button", title: "One fewer", "aria-label": "One fewer", onclick: () => {
        if (r.group) { r.group.qty--; redraw(false); return; } // back into its group
        change(r, { quantity: r.qty - 1 });
      } }, "−"),
      h("span", r.qty),
      h("button", { type: "button", title: "One more", "aria-label": "One more", onclick: () => {
        if (r.group) { // one more from the group it was split from
          const from = rowsOf(c).find((x) => x.id === `s:${r.sig}`);
          if (from && from.qty > 1) { r.group.qty++; redraw(false); }
          return;
        }
        change(r, { quantity: r.qty + 1 });
      } }, "+"));
    const fin = h("select", { "aria-label": "Finish", onchange: (e) => change(r, { finish: e.target.value }) },
      finishes.map((f) => h("option", { value: f, selected: f === r.finish }, fname(f))));
    const cond = h("select", { "aria-label": "Condition", onchange: (e) => change(r, { condition: e.target.value }) },
      state.filters.conditions.map((x) => h("option", { value: x, selected: x === r.condition }, x)));
    // Tags: chips with ×, and + tag to type one.
    const tagIn = h("input.tag-input", { type: "text", placeholder: "New tag", "aria-label": "New tag", list: "tag-names", hidden: true,
      onkeydown: (e) => {
        if (e.key === "Escape") { tagIn.hidden = true; plus.hidden = false; }
        if (e.key !== "Enter") return;
        e.preventDefault();
        const t = tidyTag(tagIn.value);
        if (!t) return;
        const problem = tagProblem(t);
        if (problem) { status(problem, true); return; }
        if ((r.tags || []).some((x) => sameTag(x, t))) { tagIn.value = ""; return; }
        change(r, { tags: [...(r.tags || []), (state.filters.tags || []).find((k) => sameTag(k, t)) || t] });
      } });
    const plus = h("button.addtag", { type: "button", onclick: () => { plus.hidden = true; tagIn.hidden = false; tagIn.focus(); } }, "+ tag");
    const where = h("div.where",
      h("span.paid", r.price_paid ? `${r.lot ? usd4(r.price_paid) : usd(r.price_paid)} each` : "no price paid", " · ", r.acquired_date || "no date",
        r.market ? ` · worth ${usd(r.market)}` : ""),
      r.lot ? h("a.lot-badge", { href: `#/collection?lot=${encodeURIComponent(r.lot)}`, title: "Show this lot's cards",
        onclick: (e) => filterBy(e, "lot", r.lot) }, "📦 ", lotName(r.lot)) : null,
      // A tag's name shows the cards with it (as the lot badge does); its × takes it off.
      (r.tags || []).map((t) => h("span.tag-chip", { style: tagStyle(t) },
        h("a.tag-name", { href: `#/collection?${new URLSearchParams({ tags: t })}`, title: `Show the cards tagged ${t} (with the filters already chosen)`,
          onclick: (e) => filterBy(e, "tags", t) }, t),
        h("button.tag-x", { type: "button", title: "Remove this tag", "aria-label": `Remove the tag ${t}`, onclick: () => change(r, { tags: r.tags.filter((x) => x !== t) }) }, "×"))),
      plus, tagIn,
      r.group ? h("span.muted.small", "Split off: change something to keep it apart") : null);
    // The inline forms: sell, and put in (or move to) a lot.
    const form = h("div.inline-form", { hidden: true });
    const showForm = (...kids) => { form.replaceChildren(...kids); form.hidden = false; };
    const cancel = h("button.link", { type: "button", onclick: () => { form.hidden = true; } }, "Cancel");
    const sell = () => {
      const n = h("input", { type: "number", min: 1, max: r.qty, step: 1, value: 1, "aria-label": "Copies sold" });
      const each = h("input", { type: "number", min: 0, step: "0.01", value: r.market ? r.market.toFixed(2) : "", placeholder: "0.00", "aria-label": "Sold for, each" });
      const fees = h("input", { type: "number", min: 0, step: "0.01", placeholder: "0.00", "aria-label": "Fees in all" });
      showForm("Sell", n, `of ${r.qty} at $`, each, "each, fees $", fees,
        h("button.primary", { type: "button", onclick: async () => {
          const q = parseInt(n.value, 10);
          if (!(q >= 1 && q <= r.qty)) { status(`Sell 1 to ${r.qty}.`, true); return; }
          if (each.value === "") { status("Type what each copy sold for.", true); each.focus(); return; }
          try {
            const s = await api("POST", gameAPI(`/card/sell?key=${encodeURIComponent(c.key)}`), {
              finish: r.finish, condition: r.condition, price_paid: r.price_paid || 0, acquired_date: r.acquired_date || "", tags: r.tags || [],
              quantity: q, sale_each: parseFloat(each.value), date: today(), fees: parseFloat(fees.value) || 0 });
            if (r.group) r.group.qty -= Math.min(q, r.group.qty);
            status(`Sold ${plural(q, "copy", "copies")} of ${c.name}${s.gain !== null ? ` (${s.gain >= 0 ? "+" : ""}${usd(s.gain)})` : ""}.`);
            await redraw();
          } catch (e) { status(e.message, true); }
        } }, "Sell"), cancel);
      n.focus();
    };
    const toLot = () => {
      const pick = h("select", { "aria-label": "Lot" }, h("option", { value: "" }, "No lot"),
        lots.map((l) => h("option", { value: l.id, selected: l.id === r.lot }, `📦 ${l.name}`)));
      showForm("Bought in", pick, h("span.muted.small", "In a lot, copies take the lot's date and their share of its price."),
        h("button.primary", { type: "button", onclick: () => {
          if (pick.value === (r.lot || "")) { form.hidden = true; return; }
          change(r, { lot: pick.value }, pick.value ? `Put ${plural(r.qty, "copy", "copies")} of ${c.name} in ${lotName(pick.value)}.` : `Took ${c.name} out of ${lotName(r.lot)}.`);
        } }, r.lot ? "Move" : "Put in lot"), cancel);
    };
    const del = async () => {
      if (r.group) { r.group.qty = 0; redraw(false); return; }
      if (!(await ask(`Delete ${plural(r.qty, "copy", "copies")} of ${c.name} (${fname(r.finish)}, ${r.condition})?`))) return;
      change(r, { quantity: 0 }, `Deleted ${plural(r.qty, "copy", "copies")} of ${c.name}.`);
    };
    const acts = h("div.copyacts",
      lots.length ? h("button", { type: "button", onclick: toLot }, r.lot ? "Move…" : "Put in lot…") : null,
      h("button", { type: "button", onclick: sell }, "Sell…"),
      r.qty > 1 ? h("button", { type: "button", title: "Move one copy to a group of its own; again, to move another into that group", onclick: () => split(r) }, "Split") : null,
      h("button.danger-text", { type: "button", onclick: del }, "Delete"));
    el.append(stepper, fin, cond, where, acts, form);
    return el;
  };

  return h("div.line-item" + (inLots.length ? ".in-lot" : ""), { "data-key": c.key },
    h("div.line",
      box,
      h("button.thumb-btn", { type: "button", "aria-label": `Open ${c.name}`, onclick: () => openCard(c.key) }, thumbnail(c) || h("span.thumb.none", c.set.toUpperCase())),
      h("div.text",
        h("button.link.name", { type: "button", onclick: () => openCard(c.key) }, c.name),
        h("span.meta", [sn(c), title(c.rarity), c.price_from === "en" ? "English price" : "", ...inLots.map((id) => `📦 ${lotName(id)}`)].filter(Boolean).join(" · "))),
      h("span.count", plural(copies, "copy", "copies"), hidden ? h("small", ` +${hidden} not shown`) : null),
      h("span.value", priced ? usd(shows ? worth : c.market_value) : "—"),
      h("button.ghost", { type: "button", title: "The card's page: printing, price watch, the full editor", onclick: () => openCard(c.key) }, "Details…")),
    h("div.copies", shown.map(rowEl),
      hidden ? h("p.muted.small.hidden-copies", `${plural(hidden, "other copy", "other copies")} of this card ${hidden === 1 ? "doesn't" : "don't"} match the lot or tag filters, so ${hidden === 1 ? "it's" : "they're"} not shown.`) : null));
}
