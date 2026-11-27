// Watched prices: what a watch's rule says, its change, and its actions.

import { api, failed, gameAPI, reload, status } from "./app.svelte";
import { signedUsd, usd } from "./format";
import type { CatalogCard } from "./types";

export interface Watch {
  key: string; card: CatalogCard; finish: string; added: string; baseline: number; baseline_date: string;
  rule: { kind: "percent" | "above" | "below"; value: number };
  triggered?: { date: string; price: number };
  price: number | null; change: number | null; changePercent: number | null;
}

export const ALERT_NOTE = "Prices are checked when the app refreshes them (each day, or as Settings says), so alerts appear while the app is open or the next time you open it. There is no email or text message.";

/** "a 20% move either way", "at or above $5.00". */
export const ruleText = (r: Watch["rule"]) => (r.kind === "percent" ? `a ${r.value}% move either way` : `${r.kind === "above" ? "at or above" : "at or below"} ${usd(r.value)}`);

/** The change since the baseline: "+$0.05 (+25%)". */
export const watchChange = (w: Watch) => (w.change === null ? "" : `${signedUsd(w.change)} (${(w.changePercent || 0) > 0 ? "+" : ""}${w.changePercent}%)`);

/** Dismisses or resets a watch's alert, or stops watching. */
export async function watchAction(w: Watch, action: "dismiss" | "reset" | "remove") {
  try {
    if (action === "remove") await api("DELETE", gameAPI(`/watchlist?key=${encodeURIComponent(w.key)}`));
    else await api("PUT", gameAPI(`/watchlist?key=${encodeURIComponent(w.key)}`), { action });
    status({ dismiss: `Dismissed the alert for ${w.card.name}.`, reset: `${w.card.name} is now measured from today's price.`, remove: `Stopped watching ${w.card.name}.` }[action]);
    reload();
  } catch (e) { failed(e); }
}
