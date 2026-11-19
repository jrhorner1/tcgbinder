// Formatting for display: money, plurals, dates, set and number, finish and language
// names.

import { app } from "./app.svelte";
import type { CatalogCard } from "./types";

export const usd = (v: number) => (v < 0 ? "-$" : "$") + Math.abs(v).toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 });

/** A gain with its sign: "+$1.05", "-$0.60". */
export const signedUsd = (v: number) => (v > 0 ? "+" : "") + usd(v);

/** A lot's price per card to four decimal places, as the lot keeps it. */
export const usd4 = (v: number | null | undefined) => (v === null || v === undefined ? "—" : "$" + v.toFixed(4));

/** Where a card is printed, e.g. "M19 · 201". */
export const sn = (c: Pick<CatalogCard, "set" | "collector_number">) => (c.collector_number ? `${c.set} · ${c.collector_number}` : c.set);

export const title = (s: string) => (s ? s[0].toUpperCase() + s.slice(1) : "");

export const plural = (n: number, one: string, many: string) => `${n.toLocaleString()} ${n === 1 ? one : many}`;

/** A finish as players say it, e.g. "nonfoil" → "Normal". */
export const fname = (f: string) => app.filters?.finishNames?.[f] || title(f || "");

/** A language code as its name, e.g. "ja" → "Japanese". */
export const languageName = (code: string) => app.filters?.languages?.find((l) => l.code === code)?.name || (code || "").toUpperCase();

/** The local date as YYYY-MM-DD (toISOString would give the UTC date). */
export const today = () => {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
};

/** One copy's price: the owned card's best, or a catalog card's first. */
export const anyPrice = (c: CatalogCard) => (c.each ? c.each : Object.values(c.market || {})[0] || 0);

/** Orders collector numbers: "2" < "10" < "10a" < "A1". */
export function compareNumber(a: string, b: string) {
  const na = parseInt(a, 10), nb = parseInt(b, 10);
  if (Number.isNaN(na) !== Number.isNaN(nb)) return Number.isNaN(na) ? 1 : -1;
  return (na - nb) || a.localeCompare(b);
}

/** A gain or loss with its percentage and color, for tiles. */
export function gain(cost: number, market: number) {
  const diff = market - cost;
  const pct = cost > 0 ? `${diff >= 0 ? "+" : ""}${((diff / cost) * 100).toFixed(1)}% vs. price paid` : "";
  return { text: signedUsd(diff), pct, color: diff >= 0 ? "var(--green)" : "var(--red)" };
}

export const choiceNames: Record<string, Record<string, string>> = {
  theme: { system: "Match system", light: "Light", dark: "Dark" },
  text_size: { small: "Small", normal: "Normal", large: "Large", larger: "Larger" },
  card_order: { name: "By name", newest: "By set, newest first", oldest: "By set, oldest first", value: "Most valuable first" },
  set_completion: { main: "The printed set (e.g. 280 cards)", all: "Every card Scryfall lists (variants too)" },
  chart_range: { "30D": "30 days", "90D": "90 days", "1Y": "1 year", All: "All time" },
  price_refresh: { daily: "Daily", weekly: "Weekly", manual: "Only when I ask" },
  default_condition: { NM: "NM — Near Mint", LP: "LP — Lightly Played", MP: "MP — Moderately Played", HP: "HP — Heavily Played", DMG: "DMG — Damaged" },
  default_date: { today: "Today", none: "Leave blank" },
  log_level: { debug: "Debug (most detail)", info: "Info", warn: "Warnings", error: "Errors only" },
};
