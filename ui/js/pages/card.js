// The card editor, opened from the collection: the card's copies and purchases,
// changing printing, and selling copies.

import { api, gameAPI, loadFilters, loadLots } from "../api.js";
import { cardImage, closeCard, copiesSummary, openCard, showWatch, tagEditor, tickInfo, ticks } from "../cards.js";
import { gain, tile } from "../charts.js";
import { $, ask, fill, h, status } from "../dom.js";
import { englishPrice, finishRank, fname, languageName, plural, signedUsd, sn, title, today, usd } from "../format.js";
import { state } from "../state.js";

// showSell sells copies from one of a card's saved purchases `l`.
function showSell(c, key, l) {
  const dlg = $("#dialog");
  const qty = h("input", { type: "number", min: 1, max: l.quantity, step: 1, value: 1, "aria-label": "Copies sold" });
  const each = h("input", { type: "number", min: 0, step: "0.01", value: l.market ? l.market.toFixed(2) : "", placeholder: "0.00", "aria-label": "Sold for, each" });
  const date = h("input", { type: "date", value: today(), "aria-label": "Date sold" });
  const fees = h("input", { type: "number", min: 0, step: "0.01", placeholder: "0.00", "aria-label": "Fees, in all" });
  const note = h("input", { type: "text", placeholder: "Where or to whom", "aria-label": "Note on the sale" });
  const outcome = h("p.note");
  const err = h("p.error");
  const recount = () => {
    const q = parseInt(qty.value, 10) || 0;
    const proceeds = q * (parseFloat(each.value) || 0) - (parseFloat(fees.value) || 0);
    outcome.textContent = l.price_paid
      ? `Paid ${usd(l.price_paid)} each. After fees: ${usd(proceeds)}, a gain of ${signedUsd(proceeds - q * l.price_paid)}.`
      : `After fees: ${usd(proceeds)}. These copies have no price paid, so the sale's gain is unknown and left out of the totals.`;
  };
  [qty, each, fees].forEach((x) => x.addEventListener("input", recount));
  recount();
  const sell = async () => {
    err.textContent = "";
    const q = parseInt(qty.value, 10);
    if (!Number.isInteger(q) || q < 1 || q > l.quantity) { err.textContent = `Sell 1 to ${l.quantity} copies.`; qty.focus(); return; }
    if (each.value === "") { err.textContent = "Type what each copy sold for."; each.focus(); return; }
    try {
      const r = await api("POST", gameAPI(`/card/sell?key=${encodeURIComponent(key)}`), {
        finish: l.finish, condition: l.condition, price_paid: l.price_paid || 0, acquired_date: l.acquired_date || "", tags: l.tags || [],
        quantity: q, sale_each: parseFloat(each.value), date: date.value, fees: parseFloat(fees.value) || 0, sale_note: note.value.trim(),
      });
      dlg.close();
      status(`Sold ${plural(q, "copy", "copies")} of ${c.name}${r.gain !== null ? `: ${signedUsd(r.gain)}` : ""}. See Tools → Earnings.`);
      await loadFilters();
      if (q >= c.copy_count) closeCard(); else openCard(key);
    } catch (e) { err.textContent = e.message; }
  };
  fill(dlg, h("div.dlg",
    h("header", h("h2", `Sell ${c.name}`), h("p.muted", { style: "margin:4px 0 0" },
      [sn(c), fname(l.finish), l.condition, plural(l.quantity, "copy", "copies"), l.acquired_date ? `acquired ${l.acquired_date}` : ""].filter(Boolean).join(" · "))),
    h("div.body",
      h("div.form",
        h("label", "Copies"), qty, h("label", "Sold for (each)"), each, h("label", "Date sold"), date,
        h("label", "Fees (in all)"), fees, h("label", "Note"), note),
      outcome,
      h("p.note", "The copies leave your collection and the sale is kept under Tools → Earnings, where it can be undone. Unsaved changes on the card page are discarded."),
      err),
    h("footer", h("button", { type: "button", onclick: () => dlg.close() }, "Cancel"), h("button.primary", { type: "button", onclick: sell }, "Sell"))));
  dlg.showModal();
  each.focus();
}

// renderCard is the card editor: the card, its value, and "Your Copies", one row per
// purchase.
export async function renderCard(container, key) {
  await loadLots();
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
      if (r.showSplit) r.showSplit();
      const q = parseInt(r.qty.value, 10) || 0;
      if (q <= 0) continue;
      n += q;
      const k = `${r.finish.value}|${r.cond.value}`;
      by.set(k, (by.get(k) || 0) + q);
    }
    const parts = [...by].map(([k, q]) => { const [f, cd] = k.split("|"); return `${fname(f)} ${cd} ×${q}`; });
    subtotal.textContent = n ? `${plural(n, "copy", "copies")}: ${parts.join(" · ")}` : "No copies: saving removes the card from your collection.";
  };
  // addRow adds a purchase row (after the row `after`, when splitting one).
  const addRow = (l, after) => {
    const r = {
      finish: h("select", { "aria-label": "Finish", onchange: update }, finishes.map((x) => h("option", { value: x, selected: x === l.finish }, fname(x)))),
      cond: h("select", { "aria-label": "Condition", onchange: update }, conds.map((x) => h("option", { value: x, selected: x === (l.condition || "NM") }, x))),
      qty: h("input", { type: "number", min: 0, step: 1, value: l.quantity ?? 1, "aria-label": "Quantity", oninput: update }),
      price: h("input", { type: "number", min: 0, step: "0.01", placeholder: "Paid each", value: l.price_paid || "", "aria-label": "Price paid each" }),
      date: h("input", { type: "date", value: l.acquired_date || "", "aria-label": "Acquired" }),
      tags: tagEditor(l.tags, { label: "Tags on these copies" }),
    };
    r.lot = l.lot || "";
    // The tags sit on their own line, folded away as "Add tags" until used.
    const noteBox = h("div.purchase-note");
    let tagsShown = !!(l.tags && l.tags.length);
    // A purchase from a lot names it; the lot sets its price.
    const lotName = r.lot ? ((state.lots || []).find((x) => x.id === r.lot) || {}).name || "a lot" : "";
    const lotLine = r.lot ? h("p.note", "From ", h("a", { href: `#/collection?lot=${encodeURIComponent(r.lot)}` }, lotName),
      ". The lot sets the price paid; a price typed here is replaced the next time the lot is repriced.") : null;
    // A saved purchase can be sold (as saved: unsaved edits to the row don't count).
    const sellButton = l.saved ? h("button.link", { type: "button", onclick: () => showSell(c, key, l) }, "Sell…") : null;
    // Split: one copy into a row of its own, to tag it apart from the rest.
    const splitButton = h("button.link", { type: "button", title: "Move one copy to a row of its own, to tag it apart", onclick: () => {
      const q = parseInt(r.qty.value, 10);
      if (!(q > 1)) return;
      let tags;
      try { tags = r.tags.value(); } catch { tags = []; }
      r.qty.value = q - 1;
      addRow({ finish: r.finish.value, condition: r.cond.value, quantity: 1, price_paid: parseFloat(r.price.value) || 0,
        acquired_date: r.date.value, lot: r.lot, tags }, r);
      rows[rows.indexOf(r) + 1].qty.focus();
    } }, "Split");
    r.showSplit = () => { splitButton.hidden = !(parseInt(r.qty.value, 10) > 1); };
    const showLine = () => fill(noteBox,
      tagsShown ? r.tags.el : h("button.link", { type: "button", onclick: () => { tagsShown = true; showLine(); r.tags.focus(); } }, "Add tags"),
      splitButton, sellButton, lotLine);
    showLine();
    r.el = h("div.purchase-row",
      h("div.purchase", r.finish, r.cond, r.qty, r.price, r.date,
        h("button", { type: "button", title: "Remove this purchase", "aria-label": "Remove this purchase", onclick: () => {
          rows.splice(rows.indexOf(r), 1); r.el.remove(); update();
        } }, "✕")),
      noteBox);
    if (after) {
      rows.splice(rows.indexOf(after) + 1, 0, r);
      after.el.after(r.el);
    } else {
      rows.push(r);
      rowsBox.append(r.el);
    }
    update();
  };
  c.purchases.forEach((l) => addRow({ ...l, saved: true }));
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
      let tags;
      try { tags = r.tags.value(); } catch (e) { err.textContent = e.message; r.tags.focus(); return; }
      purchases.push({ finish: r.finish.value, condition: r.cond.value, quantity, price_paid: r.price.value ? parseFloat(r.price.value) : 0, acquired_date: r.date.value, lot: r.lot, tags });
    }
    if (!purchases.length && !(await ask(`Remove all of ${c.name} from your collection?`, "Remove"))) return;
    try {
      const r = await api("PUT", gameAPI(`/card?key=${encodeURIComponent(key)}`), { purchases });
      status(r.deleted ? `Deleted ${c.name}` : `Saved ${c.name}`);
      if (r.deleted) closeCard(); else openCard(r.key);
    } catch (e) { err.textContent = e.message; }
  };
  const del = async () => {
    if (!(await ask(`Remove all ${c.copy_count} of ${c.name} (${sn(c)}) from your collection?`, "Remove"))) return;
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
    .map(([k, v]) => `${usd(v)} ${fname(k)}`).join(" · ") + (c.prices_updated ? `  (as of ${c.prices_updated})` : "")
    + (englishPrice(c) ? " — the English printing's price: the catalog has none for this language" : "");
  const kv = [["Set", `${c.set_name} (${c.set})`], ["Collector number", c.collector_number], ["Rarity", title(c.rarity)],
    ...(d.details || []).map((r) => [r.Label, r.Value]),
    ["Language", languageName(c.lang || "en")],
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
        d.link.url ? h("p", { style: "text-align:center" }, h("a", { href: d.link.url, target: "_blank", rel: "noopener noreferrer" }, d.link.label)) : null,
        h("p", { style: "text-align:center" }, h("button", { type: "button", onclick: () => showWatch(c) }, "Watch price…"))),
      h("div",
        h("div.tiles", { style: "grid-template-columns:repeat(auto-fit,minmax(140px,1fr))" },
          tile("MARKET VALUE", c.market_value ? usd(c.market_value) : "—", c.market_value ? [valueDet, englishPrice(c)].filter(Boolean).join(" · ") : "no market price yet", "var(--blue)"),
          tile("PRICE PAID", c.price_paid ? usd(c.price_paid) : "—", c.price_paid ? paidDet : "price paid not recorded"),
          tile("GAIN / LOSS", g ? g.text : "—", g ? g.pct : "needs both prices", g ? g.color : null)),
        h("h2", "Your Copies"),
        h("p.note", "One row per purchase, by finish and condition, with the price paid for each copy and its tags. Copies with different tags stay a separate purchase."),
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
        d.others && d.others.length ? [h("h2", "Other Printings You Own"), h("div.rows", d.others.map((o) => h("button.row", { type: "button", onclick: () => openCard(o.key) },
          h("span.name", `${o.set_name} · ${sn(o)}`), h("span.where", copiesSummary(o)))))] : null))));
}

// showPrintings lists every printing of the card; picking one moves every copy there,
// keeping each purchase's finish, condition, quantity, price paid and date.
async function showPrintings(box, key, c, lang) {
  const language = lang || c.lang || "en";
  fill(box, h("p.muted", "Finding every printing…"));
  const languages = (state.filters.languages || []);
  const langMenu = languages.length ? h("label.inline", "Language ",
    h("select", { "aria-label": "Language", onchange: (e) => showPrintings(box, key, c, e.target.value) },
      languages.map((l) => h("option", { value: l.code, selected: l.code === language }, l.name)))) : null;
  try {
    const r = await api("GET", gameAPI(`/printings?key=${encodeURIComponent(key)}&lang=${encodeURIComponent(language)}`));
    fill(box, h("h2", "Change printing"),
      h("p.note", "Pick the printing you own; every purchase moves with its condition, price paid, date and tags. A card's language belongs to its printing, so choose a language to see its printings."),
      langMenu,
      r.printings.length ? null : h("p.muted", `No printings in ${languageName(language)}.`),
      h("div.list.results", r.printings.map((p) => h("button.card" + (p.id === r.current ? ".sel" : ""), {
        type: "button",
        onclick: async () => {
          if (p.id === r.current) return;
          try {
            const res = await api("PUT", gameAPI(`/card/printing?key=${encodeURIComponent(key)}`), { id: p.id, lang: language });
            if (ticks().has(key)) { ticks().delete(key); ticks().set(res.key, tickInfo(p)); }
            status(`Moved ${c.name} to ${sn(p)}${language !== (c.lang || "en") ? ` (${languageName(language)})` : ""}${res.finishChanged ? ` (now ${res.finishes.map(fname).join(", ")}: the new printing doesn't come in every finish you had)` : ""}${res.merged ? ", merged with the copies you already had" : ""}`);
            await loadFilters();
            openCard(res.key);
          } catch (e) { status(e.message, true); }
        } },
        h("div.text", h("strong", `${p.set_name}${p.id === r.current ? "  (current)" : ""}`),
          h("span.meta", [sn(p), title(p.rarity), (p.released_at || "").slice(0, 4), (p.finishes || []).map(fname).join("/")].filter(Boolean).join(" · ")))))));
  } catch (e) { fill(box, langMenu, h("p.error", e.message)); }
}
