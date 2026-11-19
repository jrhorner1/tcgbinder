// Lots (packs, bundles, boxes, eBay lots): how far one has been entered, how it was
// counted, and finishing one.

import { api, failed, gameAPI, loadFilters, loadLots, status } from "./app.svelte";
import { askDialog } from "./dialogs.svelte";
import { plural, usd } from "./format";
import type { Lot } from "./types";

/** How much of a lot has been entered: "37 of 500 entered". */
export const lotProgress = (l: Lot) => (l.expected !== null ? `${l.entered} of ${l.expected} entered` : `${plural(l.entered, "card", "cards")} entered`);

/** How a lot was counted. */
export function lotCountText(l: Lot) {
  const c = l.count;
  if (c.kind === "known") return `${plural(c.cards, "card", "cards")}, counted`;
  if (c.kind === "bundle") return `a bundle: ${c.packs} packs × ${c.cards_per_pack}${c.extra_cards ? ` + ${c.extra_cards} other cards` : ""}${c.lands ? ` + ${c.lands} basic lands${c.lands_share_cost ? "" : " (free)"}` : ""}`;
  return "counted as they're added";
}

/** Finishes a lot: for a value split it first shows what each card will be costed at;
 *  for a short, evenly split lot it asks which count to divide by. True if finished. */
export async function finishLot(lot: Lot): Promise<boolean> {
  if (lot.split === "by_value") {
    const pct = lot.marketValue > 0 ? Math.round((lot.total_paid / lot.marketValue) * 1000) / 10 : null;
    const ok = await askDialog("Finish the lot", [`Paid ${usd(lot.total_paid)} for cards worth ${usd(lot.marketValue)} now.`,
      pct !== null ? `Each card will be costed at ${pct}% of its market price (prices from earlier days are refreshed first).` : "None of its cards has a market price, so the total will be split evenly."],
    [{ label: "Finish", value: true, primary: true }]);
    if (!ok) return false;
  }
  try {
    const path = gameAPI(`/lots?id=${encodeURIComponent(lot.id)}`);
    const r = await api("PUT", path, { open: false });
    if (r.short && r.lot.split === "even") {
      const spread = await askDialog("Fewer cards than counted",
        [`You entered ${r.short.entered} of ${r.short.expected} cards. Spread ${usd(r.lot.total_paid)} over the ${r.short.entered} entered, or keep it at ${r.short.expected}?`],
        [{ label: `Keep ${r.short.expected}`, value: "keep" }, { label: `Spread over ${r.short.entered}`, value: "spread", primary: true }]);
      if (spread === "spread") await api("PUT", path, { count: { kind: "known", cards: r.short.entered } });
    }
    await loadLots();
    await loadFilters();
    status(`Finished ${r.lot.name}.${r.note ? " " + r.note : ""}${r.outcome && r.outcome.percent !== null ? ` Each card is costed at ${r.outcome.percent}% of its market price.` : ""}`);
    return true;
  } catch (e) { failed(e); return false; }
}
