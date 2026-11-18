// Packs, bundles and lots: a lot's panel above the collection, editing a lot, and
// finishing one. Lots are added in the Add dialog (pages/add.js).

import { api, gameAPI, loadFilters, loadLots } from "../api.js";
import { gain, tile } from "../charts.js";
import { $, askDialog, fill, h, status } from "../dom.js";
import { plural, today, usd, usd4 } from "../format.js";
import { go, route } from "../nav.js";

// lotProgress says how much of a lot has been entered: "37 of 500 entered".
export const lotProgress = (l) => (l.expected !== null ? `${l.entered} of ${l.expected} entered` : `${plural(l.entered, "card", "cards")} entered`);

// lotCountText describes how a lot was counted.
function lotCountText(l) {
  const c = l.count;
  if (c.kind === "known") return `${plural(c.cards, "card", "cards")}, counted`;
  if (c.kind === "bundle") return `a bundle: ${c.packs} packs × ${c.cards_per_pack}${c.extra_cards ? ` + ${c.extra_cards} other cards` : ""}${c.lands ? ` + ${c.lands} basic lands${c.lands_share_cost ? "" : " (free)"}` : ""}`;
  return "counted as they're added";
}

// showLotDialog edits `lot` (or, with lot null, adds one), then calls done(lot).
export function showLotDialog(lot, done) {
  const l = lot || { name: "", total_paid: "", acquired_date: today(), count: { kind: "known", cards: "" }, split: "by_value", note: "" };
  const dlg = $("#dialog");
  const name = h("input", { type: "text", value: l.name, placeholder: "e.g. M19 booster, or a 500-card lot from eBay", "aria-label": "Name" });
  const total = h("input", { type: "number", min: 0, step: "0.01", value: l.total_paid, placeholder: "0.00", "aria-label": "Total paid" });
  const date = h("input", { type: "date", value: l.acquired_date, "aria-label": "Acquired" });
  const c = l.count;
  const kind = h("select", { "aria-label": "How many cards", onchange: () => { showKind(); preview(); } },
    h("option", { value: "known", selected: c.kind === "known" }, "I counted them"),
    h("option", { value: "bundle", selected: c.kind === "bundle" }, "It's a bundle"),
    h("option", { value: "as_entered", selected: c.kind === "as_entered" }, "I'll count as I add"));
  const num = (v, label, min) => h("input", { type: "number", min: min ?? 0, step: 1, value: v ?? "", "aria-label": label, oninput: () => preview() });
  const cards = num(c.kind === "known" ? c.cards : "", "Cards", 1);
  const packs = num(c.kind === "bundle" ? c.packs : 9, "Packs");
  const perPack = num(c.kind === "bundle" ? c.cards_per_pack : 14, "Cards per pack");
  const extra = num(c.kind === "bundle" ? c.extra_cards : 0, "Other cards");
  const lands = num(c.kind === "bundle" ? c.lands : 0, "Basic lands");
  const share = h("input", { type: "checkbox", checked: c.kind === "bundle" && c.lands_share_cost, onchange: () => preview() });
  const knownRows = h("div.form", h("label", "Cards"), cards);
  const bundleRows = h("div.form", h("label", "Packs"), packs, h("label", "Cards per pack"), perPack, h("label", "Other cards"), extra,
    h("label", "Basic lands"), lands, h("label", ""), h("label.check", share, "Lands share the cost"));
  const showKind = () => { knownRows.hidden = kind.value !== "known"; bundleRows.hidden = kind.value !== "bundle"; };
  const split = h("select", { "aria-label": "Split the cost", onchange: () => preview() },
    h("option", { value: "by_value", selected: l.split !== "even" }, "By each card's market value"),
    h("option", { value: "even", selected: l.split === "even" }, "Evenly"));
  const note = h("input", { type: "text", value: l.note || "", "aria-label": "Note" });
  const result = h("p.note");
  const err = h("p.error");
  const count = () => kind.value === "known" ? { kind: "known", cards: parseInt(cards.value, 10) || 0 }
    : kind.value === "bundle" ? { kind: "bundle", packs: parseInt(packs.value, 10) || 0, cards_per_pack: parseInt(perPack.value, 10) || 0,
      extra_cards: parseInt(extra.value, 10) || 0, lands: parseInt(lands.value, 10) || 0, lands_share_cost: share.checked }
    : { kind: "as_entered" };
  const preview = () => {
    const n = count();
    const divisor = n.kind === "known" ? n.cards : n.kind === "bundle" ? n.packs * n.cards_per_pack + n.extra_cards + (n.lands_share_cost ? n.lands : 0) : 0;
    const paid = parseFloat(total.value) || 0;
    if (split.value === "by_value") result.textContent = "Each card's price is worked out from its market value when you finish the lot. Until then, cards carry an even share.";
    else if (n.kind === "as_entered") result.textContent = "The total is divided by the cards entered, so every card's price changes as you add more.";
    else result.textContent = divisor > 0 ? `${plural(divisor, "card", "cards")}, $${(Math.round(paid / divisor * 10000) / 10000).toFixed(4)} each` : "";
  };
  [total].forEach((x) => x.addEventListener("input", preview));
  const save = async () => {
    err.textContent = "";
    const body = { name: name.value.trim(), total_paid: parseFloat(total.value) || 0, acquired_date: date.value, count: count(), split: split.value, note: note.value.trim() };
    try {
      const r = lot ? (await api("PUT", gameAPI(`/lots?id=${encodeURIComponent(lot.id)}`), body)).lot : await api("POST", gameAPI("/lots"), body);
      dlg.close();
      await loadLots();
      status(lot ? `Saved ${r.name}.` : `Started ${r.name}: cards you add now belong to it.`);
      done(r);
    } catch (e) { err.textContent = e.message; }
  };
  fill(dlg, h("div.dlg", h("header", h("h2", lot ? "Edit lot" : "Add a pack, bundle or lot"),
      lot ? null : h("p.muted", { style: "margin:4px 0 0" }, "Cards bought together for one price, which is split among them. A pack is usually 14 or 15 cards.")),
    h("div.body",
      h("div.form", h("label", "Name"), name, h("label", "Total paid"), total, h("label", "Acquired"), date, h("label", "How many cards?"), kind),
      knownRows, bundleRows,
      h("div.form", h("label", "Split the cost"), split, h("label", "Note"), note),
      result, err),
    h("footer", h("button", { type: "button", onclick: () => dlg.close() }, "Cancel"), h("button.primary", { type: "button", onclick: save }, lot ? "Save" : "Start adding"))));
  showKind();
  preview();
  dlg.showModal();
  name.focus();
}

// finishLot finishes a lot: for a value split it first shows what each card will be
// costed at; for a short even-split lot it asks which count to divide by.
export async function finishLot(lot, done) {
  if (lot.split === "by_value") {
    const pct = lot.marketValue > 0 ? Math.round(lot.total_paid / lot.marketValue * 1000) / 10 : null;
    const ok = await askDialog("Finish the lot", [`Paid ${usd(lot.total_paid)} for cards worth ${usd(lot.marketValue)} now.`,
      pct !== null ? `Each card will be costed at ${pct}% of its market price (prices from earlier days are refreshed first).` : "None of its cards has a market price, so the total will be split evenly."],
      [{ label: "Finish", value: true, primary: true }]);
    if (!ok) return;
  }
  try {
    const path = gameAPI(`/lots?id=${encodeURIComponent(lot.id)}`);
    const r = await api("PUT", path, { open: false });
    if (r.short && r.lot.split === "even") {
      const spread = await askDialog("Fewer cards than counted", [`You entered ${r.short.entered} of ${r.short.expected} cards. Spread ${usd(r.lot.total_paid)} over the ${r.short.entered} entered, or keep it at ${r.short.expected}?`],
        [{ label: `Keep ${r.short.expected}`, value: "keep" }, { label: `Spread over ${r.short.entered}`, value: "spread", primary: true }]);
      if (spread === "spread") await api("PUT", path, { count: { kind: "known", cards: r.short.entered } });
    }
    await loadLots();
    await loadFilters();
    status(`Finished ${r.lot.name}.${r.note ? " " + r.note : ""}${r.outcome && r.outcome.percent !== null ? ` Each card is costed at ${r.outcome.percent}% of its market price.` : ""}`);
    done(r.lot);
  } catch (e) { status(e.message, true); }
}

// #/lots (old links): the collection, filtered to the lot when there's an ID.
export function viewLots(params) {
  const id = new URLSearchParams(params).get("id");
  go("collection", id ? new URLSearchParams({ lot: id }) : null, true);
}

// lotPanel is a lot's figures and actions, shown above the collection when the list is
// filtered to it. `keepAdding` opens the Add dialog on the lot.
export function lotPanel(l, keepAdding) {
  const g = gain(l.total_paid, l.marketValue);
  const path = gameAPI(`/lots?id=${encodeURIComponent(l.id)}`);
  const act = (fn) => async () => { try { await fn(); } catch (e) { status(e.message, true); } };
  const reopen = act(async () => {
    await api("PUT", path, { open: true });
    await loadLots();
    keepAdding(l.id);
  });
  const recalc = act(async () => {
    const r = await api("PUT", path, { recalculate: true });
    status(`Repriced ${r.lot.name} at today's prices.${r.note ? " " + r.note : ""}`);
    await loadLots();
    route();
  });
  const del = act(async () => {
    const what = await askDialog(`Delete ${l.name}?`, ["Keep its cards (with the prices they have now), or delete them from your collection too?"],
      [{ label: "Delete the cards too", value: "remove", danger: true }, { label: "Keep the cards", value: "keep", primary: true }]);
    if (!what) return;
    const r = await api("DELETE", `${path}&cards=${what}`);
    status(`Deleted the lot ${l.name}${what === "remove" ? " and its cards" : ""}.`, false, r.undo);
    await loadLots();
    await loadFilters();
    go("collection");
  });
  return h("div.card-box.lot-panel",
    h("div.lot-head", h("div", h("h2", "📦 ", l.name),
      h("p.muted", [l.acquired_date, lotCountText(l), l.split === "even" ? "split evenly" : "split by market value", l.open ? "being entered" : l.split_date ? `split on ${l.split_date}` : "finished"].filter(Boolean).join(" · "))),
    h("div.buttons",
      l.open ? h("button.primary", { type: "button", onclick: () => keepAdding(l.id) }, "Keep adding") : h("button", { type: "button", onclick: reopen }, "Add more"),
      l.open ? h("button", { type: "button", onclick: () => finishLot(l, () => route()) }, "Finish") : null,
      !l.open && l.split === "by_value" ? h("button", { type: "button", onclick: recalc }, "Reprice at today's prices") : null,
      h("button", { type: "button", onclick: () => showLotDialog(l, async () => { await loadLots(); route(); }) }, "Edit"),
      h("button.danger", { type: "button", onclick: del }, "Delete"))),
    h("div.tiles",
      tile("PAID", usd(l.total_paid), l.priceEach !== null ? `${usd4(l.priceEach)} each` : plural(l.copies, "card", "cards")),
      tile("WORTH NOW", usd(l.marketValue), plural(l.copies, "card", "cards"), "var(--blue)"),
      tile("ENTERED", l.expected !== null ? `${l.entered} / ${l.expected}` : l.entered.toLocaleString(), lotProgress(l)),
      tile("GAIN / LOSS", g.text, g.pct, g.color)));
}
