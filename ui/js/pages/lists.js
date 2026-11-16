// Lists and decks: every list, and one list with what is owned and missing.

import { api, gameAPI } from "../api.js";
import { addInDialog, thumbnail } from "../cards.js";
import { tile } from "../charts.js";
import { $, askDialog, askName, confirmDialog, fill, h, status, whileRunning } from "../dom.js";
import { fname, plural, sn, usd } from "../format.js";
import { go, route } from "../nav.js";

// viewLists shows every list, or one list (`id`).
export async function viewLists(qs) {
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

// viewList is one list: totals, entries (owned of wanted, price), and its actions.
export async function viewList(id) {
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
