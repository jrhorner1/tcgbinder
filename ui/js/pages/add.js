// The Add page: search the catalog and add a card, paste a list, or start a pack,
// bundle or lot, with the cards added this session.

import { api, gameAPI, loadFilters, loadLots } from "../api.js";
import { pickList, sameTag } from "../cards.js";
import { $, fill, h, status, whileRunning } from "../dom.js";
import { fname, gameInfo, plural, usd, usd4 } from "../format.js";
import { state } from "../state.js";
import { finishLot, lotProgress, showLotDialog } from "./lots.js";
import { showImportPreview } from "./tools.js";

// Add: search the game's catalog, pick the exact printing, set ownership details.
// The page stays open after adding, so many cards can be added in a row; each is
// listed under "Added this session" with an Undo.
export function viewAdd(params) {
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
        h("div.add-kinds", h("span.muted", "Add:"),
          h("button.on", { type: "button", "aria-pressed": "true", onclick: () => text.focus() }, "A single card"),
          h("button", { type: "button", onclick: startLot }, "A pack, bundle or lot…"),
          h("a", { href: "#/lots" }, "All bundles/lots")));
      return;
    }
    fill(banner, h("div.card-box.notice.lot-banner",
      h("div", h("strong", lot.name), h("div.muted", [lotProgress(lot), lot.split === "even" ? `${usd4(lot.priceEach)} each` : `worth ${usd(lot.marketValue)} so far`].join(" · ")),
        h("div", h("a", { href: `#/lots?id=${encodeURIComponent(lot.id)}` }, "This lot's page"), " · ", h("a", { href: "#/lots" }, "All lots"))),
      h("div.right", h("button", { type: "button", onclick: () => { state.activeLot = null; lot = null; showBanner(); refreshPicker(); } }, "Pause"),
        h("button.primary", { type: "button", onclick: () => finishLot(lot, () => { state.activeLot = null; lot = null; showBanner(); refreshPicker(); }) }, "Finish"))));
  };

  // A new pack or lot: the cards added next belong to it.
  const startLot = () => showLotDialog(null, (l) => { state.activeLot = l.id; lot = l; showBanner(); refreshPicker(); });

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
  // From the Add menu: "A pack, bundle or lot…" opens its dialog straight away.
  if (pre.get("new") === "lot") {
    history.replaceState(null, "", "#/add");
    startLot();
  }
  showSession();
  loadLots().then(() => { if (lot) lot = (state.lots || []).find((l) => l.id === lot.id) || lot; showBanner(); });
  if (state.info.client !== "phone") text.focus(); // on phones the keyboard would cover the page
  if (pre.get("text") || pre.get("set")) run(false);
}
