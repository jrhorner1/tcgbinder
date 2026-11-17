// The Add dialog: add a card, or a pack, bundle or lot (a booster box, a bundle, an
// eBay lot...) and then the cards in it, all in one dialog. Opened from the sidebar,
// the phone's Add tab, the search at the top and an unfinished lot's banner.

import { api, gameAPI, loadLots } from "../api.js";
import { addCard, addDefaults, tagEditor, thumbnail } from "../cards.js";
import { $, fill, h, status, whileRunning } from "../dom.js";
import { fname, plural, sn, title, today, usd, usd4 } from "../format.js";
import { go, route } from "../nav.js";
import { state } from "../state.js";
import { finishLot, lotProgress } from "./lots.js";
import { showImportPreview } from "./tools.js";

// The standard contents the lot form's small buttons fill in: packs × cards per pack,
// other cards (a promo, a toolkit's extra cards) and basic lands.
const PRESETS = [
  { label: "Booster pack", packs: 1, per: 14, extra: 0, lands: 0 },
  { label: "Booster box", packs: 36, per: 14, extra: 0, lands: 0 },
  { label: "Collector booster box", packs: 12, per: 15, extra: 0, lands: 0 },
  { label: "Bundle", packs: 9, per: 14, extra: 1, lands: 40 },
  { label: "Prerelease kit", packs: 6, per: 14, extra: 1, lands: 0 },
  { label: "Deck builder's toolkit", packs: 4, per: 14, extra: 125, lands: 100 },
];

// #/add (the phone's Add tab, old links): the collection, with the Add dialog over it.
export async function viewAdd(params) {
  const pre = new URLSearchParams(params || "");
  history.replaceState(null, "", "#/collection");
  await route();
  openAdd({ query: pre.get("text") || "", lot: pre.get("lot") || "" });
}

// openAdd opens the Add dialog: adding a card, or entering the cards of the open lot
// `lot` (its ID). `query` (with `set`, a set code) searches for a card straight away.
export function openAdd({ lot = "", query = "", set = "" } = {}) {
  const dlg = $("#add-dialog");
  let current = lot ? (state.lots || []).find((l) => l.id === lot) || null : null;
  let mode = current ? "entering" : "card";
  let lastQuery = query;
  let lastSet = set; // the set asked for, until the search text changes
  let changed = false;
  const added = [];
  // The card fields that stay from one add to the next.
  const d = addDefaults();
  const cardPrice = h("input", { type: "number", min: 0, step: "0.01", placeholder: "0.00", value: d.price || "", "aria-label": "Price paid each" });
  const cardDate = h("input", { type: "date", value: d.date || "", "aria-label": "Acquired" });
  const cardTags = tagEditor(d.tags || [], { label: "Tags on the cards added" });

  // The catalog search: results with a quantity, finish, condition and Add each. The
  // text stays as cards are added, so the next copy is one click away.
  function search(onAdd) {
    const [textHint] = state.filters.searchHints || ["Card name"];
    const text = h("input", { type: "search", placeholder: `${textHint}, or a name and set code`, value: lastQuery, "aria-label": "Search for a card", autocomplete: "off" });
    const list = h("div.add-results");
    let next = "", cards = [], owned = [], asked = 0, timer;
    const show = () => fill(list,
      cards.length ? cards.map((c, i) => resultRow(c, owned[i], onAdd))
        : h("p.muted.empty", lastQuery.trim() ? "No cards found." : "Search for a card by name, or a name and set code (e.g. Shock M19)."),
      next ? h("button", { type: "button", onclick: () => run(true) }, "More results") : null);
    const run = async (more) => {
      const q = text.value.trim();
      if (!q) { cards = []; owned = []; next = ""; show(); return; }
      const mine = ++asked;
      try {
        // "Shock M19" or "Opt DOM": a last word with a digit, or typed in capitals,
        // is a set code.
        const words = q.split(/\s+/);
        const last = words.at(-1);
        const looksLikeSet = words.length > 1 && /^[a-z0-9]{3,5}$/i.test(last) && (/\d/.test(last) || last === last.toUpperCase());
        const setCode = lastSet || (looksLikeSet ? words.pop() : "");
        const p = new URLSearchParams({ text: words.join(" "), set: setCode });
        if (more && next) p.set("next", next);
        const r = await api("GET", gameAPI(`/search?${p}`));
        if (mine !== asked) return; // a later search is on its way
        cards = more ? cards.concat(r.cards) : r.cards;
        owned = more ? owned.concat(r.owned) : r.owned;
        next = r.next;
        show();
      } catch (e) { if (mine === asked) fill(list, h("p.error", e.message)); }
    };
    text.addEventListener("input", () => {
      lastQuery = text.value;
      lastSet = "";
      clearTimeout(timer);
      if (text.value.trim().length >= 3) timer = setTimeout(() => run(false), 400);
    });
    text.addEventListener("keydown", (e) => { if (e.key === "Enter") { e.preventDefault(); clearTimeout(timer); run(false); } });
    show();
    if (lastQuery.trim()) run(false);
    return { el: h("div.add-search", text, list), focus: () => text.focus() };
  }

  function resultRow(c, own, onAdd) {
    const qty = h("input", { type: "number", min: 1, step: 1, value: 1, "aria-label": `How many ${c.name}` });
    const finishes = c.finishes && c.finishes.length ? c.finishes : ["nonfoil"];
    const fin = h("select", { "aria-label": "Finish" }, finishes.map((f) => h("option", { value: f, selected: f === addDefaults().finish }, fname(f))));
    const cond = h("select", { "aria-label": "Condition" },
      state.filters.conditions.map((x) => h("option", { value: x, selected: x === (addDefaults().condition || "NM") }, x)));
    const market = c.market || {};
    const price = market[fin.value] ?? Object.values(market)[0];
    const button = h("button.primary", { type: "button", onclick: async () => {
      button.disabled = true;
      try { await onAdd(c, { quantity: parseInt(qty.value, 10) || 1, finish: fin.value, condition: cond.value }); qty.value = 1; } catch (e) { status(e.message, true); }
      button.disabled = false;
    } }, "Add");
    return h("div.add-result", thumbnail(c),
      h("div.text", h("span.name", c.name), h("span.meta", [sn(c), title(c.rarity), price ? usd(price) : "", own ? `own ${own}` : ""].filter(Boolean).join(" · "))),
      h("div.controls", qty, fin, cond, button));
  }

  // Paste a list: the import preview, into the lot when entering one.
  function pastePanel() {
    const box = h("textarea", { rows: 6, placeholder: "4 Lightning Bolt\n1 Sol Ring (C21) 263\n2 Shock [M19] *F*", "aria-label": "Card list" });
    const out = h("div");
    const panel = h("div.paste", { hidden: true },
      h("p.muted.small", "One card per line: a count, the name, and optionally the set and collector number. Add *F* for foil."), box,
      h("div.buttons", h("button", { type: "button", onclick: async () => {
        if (!box.value.trim()) { status("Paste a list first."); return; }
        try {
          const p = await whileRunning(api("POST", gameAPI("/import?preview=1"), new Blob([box.value], { type: "text/plain" })));
          const into = mode === "entering" ? current : null;
          showImportPreview(out, p, into ? `pasted list for ${into.name}` : "pasted list", () => fill(out), into,
            (r) => { changed = true; if (r.lot) { current = r.lot; render(); } });
        } catch (e) { status(e.message, true); }
      } }, "Preview")), out);
    const toggle = h("button.link", { type: "button", onclick: () => { panel.hidden = !panel.hidden; if (!panel.hidden) box.focus(); } }, "Paste a list…");
    return [toggle, panel];
  }

  const addedList = () => (added.length ? h("div.added", h("h3", "Added just now"), h("ul", added.slice().reverse().map((a) => h("li", a)))) : null);
  const said = (c, a) => `${a.quantity}× ${c.name} (${sn(c)}, ${fname(a.finish)}, ${a.condition})`;

  function cardForm() {
    const s = search(async (c, a) => {
      let tags;
      try { tags = cardTags.value(); } catch (e) { status(e.message, true); return; }
      await addCard(c, { ...a, price: cardPrice.value, date: cardDate.value, tags });
      changed = true;
      added.push(said(c, a));
      status(`Added ${a.quantity}× ${c.name}.`);
      render();
    });
    const [pasteToggle, paste] = pastePanel();
    setTimeout(s.focus, 0);
    return {
      heading: "Add a card",
      body: [h("div.form.add-defaults", h("label", "Price paid each"), cardPrice, h("label", "Acquired"), cardDate, h("label", "Tags"), cardTags.el),
        s.el, h("p", pasteToggle), paste, addedList()],
      foot: [h("button.primary", { type: "button", onclick: () => dlg.close() }, "Done")],
    };
  }

  // The lot form starts blank; the small buttons fill in a product's standard contents.
  function lotForm() {
    const num = (label, placeholder) => h("input", { type: "number", min: 0, step: 1, placeholder, "aria-label": label });
    const name = h("input", { type: "text", placeholder: "e.g. Bloomburrow bundle, or a 500-card lot from eBay", "aria-label": "Name" });
    const total = h("input", { type: "number", min: 0, step: "0.01", placeholder: "0.00", "aria-label": "Total paid" });
    const date = h("input", { type: "date", value: today(), "aria-label": "Bought" });
    const packs = num("Packs", "0"), per = num("Cards per pack", "14"), extra = num("Other cards", "0"), lands = num("Basic lands", "0");
    const split = h("select", { "aria-label": "Split the cost" },
      h("option", { value: "by_value" }, "By each card's market value"), h("option", { value: "even" }, "Evenly"));
    const sum = h("span.muted.small");
    const err = h("p.error");
    const n = (x) => parseInt(x.value, 10) || 0;
    const cards = () => n(packs) * (n(per) || 14) + n(extra);
    const recount = () => { sum.textContent = `= ${plural(cards(), "card", "cards")}${n(lands) ? `, and ${n(lands)} basic lands that don't share the cost` : ""}`; };
    [packs, per, extra, lands].forEach((x) => x.addEventListener("input", recount));
    recount();
    const presets = h("div.presets", h("span.muted.small", "Fill in:"), PRESETS.map((p) => h("button", { type: "button", onclick: (e) => {
      packs.value = p.packs; per.value = p.per; extra.value = p.extra; lands.value = p.lands;
      if (!name.value.trim()) name.value = p.label;
      presets.querySelectorAll("button").forEach((b) => b.classList.toggle("on", b === e.currentTarget));
      err.textContent = "";
      recount();
    } }, p.label)));
    const start = async () => {
      err.textContent = "";
      if (!name.value.trim()) { err.textContent = "Give it a name."; name.focus(); return; }
      if (cards() < 1) { err.textContent = "Say what's in it: packs or other cards."; packs.focus(); return; }
      const count = n(packs) || n(lands)
        ? { kind: "bundle", packs: n(packs), cards_per_pack: n(per) || 14, extra_cards: n(extra), lands: n(lands), lands_share_cost: false }
        : { kind: "known", cards: n(extra) };
      try {
        current = await api("POST", gameAPI("/lots"), { name: name.value.trim(), total_paid: parseFloat(total.value) || 0, acquired_date: date.value, count, split: split.value });
        await loadLots();
        changed = true;
        mode = "entering";
        render();
      } catch (e) { err.textContent = e.message; }
    };
    setTimeout(() => name.focus(), 0);
    return {
      heading: "Add",
      body: [presets,
        h("div.form", h("label", "Name"), name, h("label", "Total paid"), total, h("label", "Bought"), date,
          h("label", "What's in it"), h("div.counts", packs, h("span", "packs ×"), per, h("span", "+"), extra, h("span", "other cards +"), lands, h("span", "basic lands"), sum),
          h("label", "Split the cost"), split), err],
      foot: [h("button", { type: "button", onclick: () => dlg.close() }, "Cancel"), h("button.primary", { type: "button", onclick: start }, "Start entering cards")],
    };
  }

  // Entering a lot's cards: its progress, the search, Pause and Finish.
  function lotEntry() {
    const l = current;
    const each = l.priceEach !== null && l.priceEach !== undefined ? `${usd4(l.priceEach)} each until it's finished` : `worth ${usd(l.marketValue)} so far`;
    const pct = l.expected ? Math.min(100, (l.entered / l.expected) * 100) : 0;
    const s = search(async (c, a) => {
      const r = await addCard(c, { ...a, lot: l.id, tags: [] });
      changed = true;
      if (r.lot) current = r.lot;
      added.push(said(c, a));
      render();
    });
    const [pasteToggle, paste] = pastePanel();
    setTimeout(s.focus, 0);
    return {
      heading: "Add cards to a lot",
      body: [h("div.lot-progress", h("strong", "📦 ", l.name), h("div.muted.small", `${lotProgress(l)} · ${each}`),
        l.expected ? h("div.bar", h("i", { style: `width:${pct}%` })) : null),
      s.el, h("p", pasteToggle), paste, addedList()],
      foot: [h("button", { type: "button", onclick: () => { dlg.close(); status(`Paused ${l.name}: keep adding from its banner in the collection.`); } }, "Pause"),
        h("button.primary", { type: "button", onclick: () => finishLot(l, () => { changed = true; dlg.close(); }) }, "Finish")],
    };
  }

  function render() {
    const view = mode === "card" ? cardForm() : mode === "lot" ? lotForm() : lotEntry();
    // The lot button: in the lot form it shows as chosen, with × to go back to a card.
    const lotButton = mode === "card"
      ? h("button", { type: "button", onclick: () => { mode = current && current.open ? "entering" : "lot"; render(); } }, "📦 A pack, bundle or lot…")
      : mode === "lot"
        ? h("span.toggle", h("button.on", { type: "button", "aria-pressed": "true" }, "📦 A pack, bundle or lot"),
          h("button.x", { type: "button", title: "Back to adding a card", "aria-label": "Back to adding a card", onclick: () => { mode = "card"; render(); } }, "×"))
        : null;
    fill(dlg, h("div.dlg.add-dlg", h("header", h("div.addhead", h("h2", view.heading), lotButton)), h("div.body", view.body), h("footer", view.foot)));
  }

  render();
  if (!dlg.open) dlg.showModal();
  dlg.addEventListener("close", async () => {
    if (!changed) return;
    await loadLots();
    if (location.hash.startsWith("#/collection")) route();
    else go("collection");
  }, { once: true });
}
