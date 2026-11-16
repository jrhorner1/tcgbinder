// Formatting for display: money, plurals, dates, set and number, finish and language
// names. Pure functions, apart from reading the game's names from state.

import { state } from "./state.js";

export const usd = (v) => (v < 0 ? "-$" : "$") + Math.abs(v).toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 });
// sn shows where a card is printed, e.g. "M19 · 201".

export const sn = (c) => (c.collector_number ? `${c.set} · ${c.collector_number}` : c.set);

export const title = (s) => (s ? s[0].toUpperCase() + s.slice(1) : "");
// fname shows a finish as players say it, e.g. "nonfoil" → "Normal".

export const fname = (f) => (state.filters && state.filters.finishNames && state.filters.finishNames[f]) || title(f || "");

export const plural = (n, one, many) => `${n.toLocaleString()} ${n === 1 ? one : many}`;
// today is the local date as YYYY-MM-DD (toISOString would give the UTC date).

export const today = () => { const d = new Date(); return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`; };

export const gameInfo = () => state.info.games.find((g) => g.ID === state.game) || state.info.games[0];

export const shortName = (name) => name.split(":")[0];

export const finishRank = (f) => { const i = state.filters.finishes.indexOf(f); return i < 0 ? 99 : i; };

export const anyPrice = (c) => (c.each ? c.each : Object.values(c.market || {})[0] || 0);
// languageName shows a language code as its name, e.g. "ja" → "Japanese".

export const languageName = (code) => ((state.filters && state.filters.languages) || []).find((l) => l.code === code)?.name || (code || "").toUpperCase();
// noted marks a card with a note on any of its purchases.

export const noted = (c) => ((c.purchases || []).some((p) => p.note) ? "has a note" : null);
// englishPrice marks a card valued at its English printing's price (the catalog has

// none for its own language).
export const englishPrice = (c) => (c.price_from === "en" ? "English price" : null);

// signedUsd shows a gain with its sign: "+$1.05", "-$0.60".
export const signedUsd = (v) => (v > 0 ? "+" : "") + usd(v);

// compareNumber orders collector numbers: "2" < "10" < "10a" < "A1".
export function compareNumber(a, b) {
  const na = parseInt(a, 10), nb = parseInt(b, 10);
  if (Number.isNaN(na) !== Number.isNaN(nb)) return Number.isNaN(na) ? 1 : -1;
  return (na - nb) || a.localeCompare(b);
}

// usd4 shows a lot's price per card to four decimal places, as the lot keeps it.
export const usd4 = (v) => (v === null || v === undefined ? "—" : "$" + v.toFixed(4));

export const choiceNames = {
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
