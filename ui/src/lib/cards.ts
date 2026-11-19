// About cards, for several pages: a compact summary of the copies owned, and adding
// copies of a catalog card (remembering the details for the next one).

import { api, app, filters, gameAPI, loadFilters, settings } from "./app.svelte";
import { fname, today } from "./format";
import type { Card, CatalogCard } from "./types";

/** The copies owned, compactly: "×3" when they're all in the usual finish and near
 *  mint, otherwise per finish, e.g. "Normal ×2 · Foil ×1 (1 LP)". */
export function copiesSummary(c: Card) {
  const f = filters();
  const byFinish = new Map<string, { n: number; worn: Map<string, number> }>();
  for (const p of c.purchases || []) {
    const x = byFinish.get(p.finish) || { n: 0, worn: new Map() };
    x.n += p.quantity;
    if (p.condition !== "NM") x.worn.set(p.condition, (x.worn.get(p.condition) || 0) + p.quantity);
    byFinish.set(p.finish, x);
  }
  const rank = (k: string) => { const i = f.finishes.indexOf(k); return i < 0 ? 99 : i; };
  const finishes = [...byFinish.keys()].sort((a, b) => rank(a) - rank(b) || a.localeCompare(b));
  if (finishes.length === 1 && finishes[0] === f.finishes[0] && !byFinish.get(finishes[0])!.worn.size) return `×${c.copy_count}`;
  return finishes.map((k) => {
    const x = byFinish.get(k)!;
    const worn = [...x.worn].sort((a, b) => f.conditions.indexOf(a[0]) - f.conditions.indexOf(b[0])).map(([cond, n]) => `${n} ${cond}`);
    return `${fname(k)} ×${x.n}${worn.length ? ` (${worn.join(", ")})` : ""}`;
  }).join(" · ");
}

export interface AddDetails {
  quantity: number;
  finish: string;
  condition: string;
  price: string;
  date: string;
  lot?: string;
  tags?: string[];
}

/** The details of the last card added, so a stack from one purchase is typed once. */
let last: Partial<AddDetails> | null = null;

export function addDefaults(): Partial<AddDetails> {
  return last || { finish: "", condition: settings().default_condition, price: "", date: settings().default_date === "today" ? today() : "", tags: [] };
}

/** Adds copies of a catalog card and remembers the details for the next one. */
export async function addCard(c: CatalogCard, d: AddDetails) {
  const r = await api("POST", gameAPI("/cards"), {
    card: c, quantity: d.quantity, finish: d.finish, condition: d.condition,
    price_paid: d.price ? parseFloat(d.price) : 0, acquired_date: d.date, lot: d.lot || "", tags: d.tags || [],
  });
  // In a lot the price and date are the lot's, so those aren't remembered.
  last = d.lot ? { ...addDefaults(), finish: d.finish, condition: d.condition } : { finish: d.finish, condition: d.condition, price: d.price, date: d.date, tags: d.tags };
  if ((d.tags || []).length) await loadFilters();
  return r as { key: string; newCards: number; alreadyOwned: number; lot?: unknown };
}

/** The game shown, as /api/info names it. */
export const gameInfo = () => app.info!.games.find((g) => g.ID === app.game) || app.info!.games[0];
