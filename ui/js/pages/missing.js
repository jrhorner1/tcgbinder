// A set's missing cards: the cards of a set you don't own yet, on a page of their own
// (from the set page in the collection), and making a list of them.

import { api, gameAPI } from "../api.js";
import { addInDialog, addToList } from "../cards.js";
import { $, askName, fill, h, status } from "../dom.js";
import { anyPrice, compareNumber, plural, title, usd } from "../format.js";
import { go } from "../nav.js";

// createMissingList asks for a name ("Core Set 2019 Missing" to start with), then puts
// one of each card of set `code` that you don't own on a new list, and opens it. The
// list takes cards off as they join the collection.
// `m` is the missing cards when they're already loaded.
export async function createMissingList(code, setName, m) {
  const name = await askName("Create a list of the missing cards", `${setName || code.toUpperCase()} Missing`);
  if (!name) return;
  try {
    status("Loading the set's cards…");
    const missing = m || (await api("GET", gameAPI(`/missing?set=${encodeURIComponent(code)}`)));
    if (!missing.missing.length) { status("You have every card in this set, so there's nothing to put on a list."); return; }
    // A missing list crosses cards off as you add them (remove_owned).
    const list = await api("POST", gameAPI("/lists"), { name, remove_owned: true });
    await api("PUT", gameAPI(`/lists?id=${encodeURIComponent(list.id)}`),
      { entries: missing.missing.map((c) => ({ card: c, quantity: 1, any_printing: false, finish: "" })) });
    status(`Made the list ${name} with ${plural(missing.missing.length, "card", "cards")}.`);
    go("lists", new URLSearchParams({ id: list.id }));
  } catch (e) { status(e.message, true); }
}

// The missing cards of the set in `?set=`, by number or most expensive first. Each can
// go on a list or be added; clicking one searches its printings.
export async function viewMissing(params) {
  const code = new URLSearchParams(params).get("set") || "";
  fill($("#main"), h("p.muted", "Loading the set's cards…"));
  const [m, d] = await Promise.all([api("GET", gameAPI(`/missing?set=${encodeURIComponent(code)}`)),
    api("GET", gameAPI(`/set?code=${encodeURIComponent(code)}`))]);
  const setName = d.name || code.toUpperCase();
  const byNumber = (a, b) => compareNumber(a.collector_number, b.collector_number);
  const byPrice = (a, b) => (anyPrice(b) || 0) - (anyPrice(a) || 0) || byNumber(a, b);
  const sortMenu = h("select", { "aria-label": "Sort the missing cards", onchange: () => show() },
    h("option", { value: "number" }, "By number"), h("option", { value: "price" }, "Most expensive first"));
  const list = h("div.list.missing");
  const show = () => fill(list, [...m.missing].sort(sortMenu.value === "price" ? byPrice : byNumber).map((c) => {
    const row = h("div.card",
      h("a.rowlink", { href: `#/add?${new URLSearchParams({ text: c.name, set: code })}` },
        h("span.num", c.collector_number), h("span.text", h("span.name", c.name),
          h("span.meta", [title(c.rarity), anyPrice(c) ? usd(anyPrice(c)) : ""].filter(Boolean).join(" · ")))),
      h("div.row-actions", h("button", { type: "button", onclick: () => addToList(c, 1) }, "To list…"),
        h("button", { type: "button", onclick: () => addInDialog(c, () => row.remove()) }, "Add…")));
    return row;
  }));
  fill($("#main"),
    h("p", h("a", { href: `#/collection?${new URLSearchParams({ set: code })}` }, `← ${setName}`)),
    h("h1", `${setName}: missing cards`),
    m.missing.length ? [
      h("p.muted", `${plural(m.missing.length, "card", "cards")} you don't own yet${m.printedSize ? ` (of the ${m.printedSize} printed)` : ""}: ${usd(m.totalCost)} for one of each${m.unpriced ? `, ${m.unpriced} without a price` : ""}. Add one here, or click it to search its printings.`),
      h("div.buttons.missing-actions", sortMenu,
        h("button.primary", { type: "button", onclick: () => createMissingList(code, setName, m) }, "Create a list of missing cards…")),
      list,
    ] : h("p.muted", "You have every card in this set."));
  show();
}
