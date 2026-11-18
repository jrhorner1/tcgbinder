// Bulk edit: changing or deleting the ticked cards at once.

import { api, gameAPI, loadFilters, loadLots } from "../api.js";
import { sortTags, tagEditor, ticks } from "../cards.js";
import { $, ask, confirmDialog, fill, h, status } from "../dom.js";
import { choiceNames, fname, plural } from "../format.js";
import { route } from "../nav.js";
import { state } from "../state.js";

// showBulkEdit changes or deletes every copy of the ticked cards.
export function showBulkEdit() {
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
  const addTags = tagEditor([], { label: "Tags to add" });
  // Only the tags the ticked cards have can be removed.
  const onTicked = sortTags([...new Map([...chosen.values()].flatMap((c) => c.tags || []).map((t) => [t.toLowerCase(), t])).values()]);
  const removeTags = tagEditor([], { label: "Tags to remove", suggest: onTicked });
  const clearTags = h("input", { type: "checkbox" });
  const lotMenu = h("select", { "aria-label": "Put in a lot" }, h("option", { value: "" }, "Unchanged"),
    (state.lots || []).map((x) => h("option", { value: x.id }, x.name)));
  const err = h("p.error");
  const apply = async () => {
    err.textContent = "";
    if (!chosen.size) { err.textContent = "Tick at least one card."; return; }
    let add_tags, remove_tags;
    try { add_tags = addTags.value(); remove_tags = removeTags.value(); } catch (e) { err.textContent = e.message; return; }
    if (lotMenu.value) {
      // Putting cards in a lot is a change of its own: the lot sets their price and date.
      if (del.checked || cond.value || fin.value || date.value || price.value !== "" || add_tags.length || remove_tags.length || clearTags.checked) {
        err.textContent = "Put in a lot on its own: set the other changes back to Unchanged, or do them after."; return;
      }
      const lot = state.lots.find((x) => x.id === lotMenu.value);
      const n = plural(chosen.size, "card", "cards");
      if (!(await confirmDialog(`Put every copy of ${n} in ${lot.name}? Their price paid and date come from the lot, and copies in another lot leave it.`, { action: "Put in lot" }))) return;
      try {
        const r = await api("POST", gameAPI(`/lots/cards?id=${encodeURIComponent(lot.id)}`), { cards: [...chosen.keys()].map((key) => ({ key })) });
        status(`Put ${plural(r.moved, "copy", "copies")} in ${lot.name}.`, false, r.undo);
        ticked.clear();
        dlg.close();
        await loadLots();
        route();
      } catch (e) { err.textContent = e.message; }
      return;
    }
    const body = { keys: [...chosen.keys()], condition: cond.value, finish: fin.value, acquired_date: date.value, delete: del.checked,
      add_tags, remove_tags, clear_tags: clearTags.checked };
    if (price.value !== "") body.price_paid = parseFloat(price.value);
    if (!body.delete && !body.condition && !body.finish && !body.acquired_date && body.price_paid === undefined
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
        h("label", "Put in a lot"), (state.lots || []).length ? lotMenu : h("span.muted", "No lots yet."),
        h("label", "Add tags"), addTags.el,
        h("label", "Remove tags"), onTicked.length ? removeTags.el : h("span.muted", "The ticked cards have no tags."),
        h("label", ""), h("label.check", { style: "display:flex;gap:8px;align-items:center;text-align:left" }, clearTags, "Remove all tags"),
        h("label", ""), h("label.check", { style: "display:flex;gap:8px;align-items:center;color:var(--red);text-align:left" }, del, "Delete these cards")),
      h("p.note", "Changes apply to every copy of each card above. Tags are changed in this order: Remove all tags, then Remove tags, then Add tags. Finish changes skip printings that don't come in that finish; purchases that end up the same combine."),
      err),
    h("footer", h("button", { type: "button", onclick: () => { dlg.close(); route(); } }, "Cancel"), h("button.primary", { type: "button", onclick: apply }, "Apply"))));
  dlg.showModal();
}
