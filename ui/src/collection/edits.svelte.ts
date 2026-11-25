// Editing the collection table: the rows of copies each card shows, splitting copies
// off into a group of their own, and saving a change (with Undo).
//
// The app keeps identical copies together, so copies split off a group stay the same
// as it until something about them changes. Until then the new group lives here (and
// is gone after a reload); a change to it saves it.

import { api, gameAPI, loadFilters, loadLots, reload, status } from "../lib/app.svelte";
import { ask } from "../lib/dialogs.svelte";
import { plural } from "../lib/format";
import type { Card, Purchase } from "../lib/types";

/** A purchase's fields that can change. */
export type Fields = Pick<Purchase, "finish" | "condition" | "quantity" | "price_paid" | "acquired_date" | "lot" | "tags">;

/** A row of copies as the table shows it: a saved purchase, or a group split off one
 *  and not saved yet (`group`). */
export interface Row extends Fields {
  id: string;
  sig: string;
  market: number;
  group?: Group;
}

interface Group { id: number; sig: string; row: Omit<Row, "id" | "quantity" | "group" | "sig">; qty: number }

// Split groups not saved yet, per card key.
const pending = new Map<string, Group[]>();
// The group each row's last split made (by row ID), so more splits add to it.
const splitInto = new Map<string, number>();
// The order rows were first shown in, per card key (by signature): new groups go at
// the end and changed rows keep their place, for as long as the app is open.
const order = new Map<string, string[]>();
let nextGroup = 1;

/** Bumped when split groups change, so the table redraws. */
export const splits = $state({ version: 0 });

/** What makes copies one group: everything but how many. */
const sig = (p: Omit<Fields, "quantity">) =>
  [p.finish, p.condition, p.price_paid || 0, p.acquired_date || "", p.lot || "", p.tags.map((t) => t.toLowerCase()).sort().join(";")].join("|");

/** The rows card `c` shows: its saved purchases (less copies split off them) in the
 *  order they were first shown, then the split groups not saved yet. */
export function rowsOf(c: Card): Row[] {
  void splits.version;
  const groups = (pending.get(c.key) || []).filter((g) => g.qty > 0);
  pending.set(c.key, groups);
  const saved = c.purchases.map((p) => {
    const s = sig(p);
    const taken = groups.filter((g) => g.sig === s).reduce((n, g) => n + g.qty, 0);
    return { ...p, tags: [...p.tags], sig: s, id: `s:${s}`, quantity: p.quantity - taken } as Row;
  });
  const seen = (order.get(c.key) || []).filter((s) => saved.some((r) => r.sig === s));
  for (const r of saved) if (!seen.includes(r.sig)) seen.push(r.sig);
  order.set(c.key, seen);
  saved.sort((a, b) => seen.indexOf(a.sig) - seen.indexOf(b.sig));
  return [...saved.filter((r) => r.quantity > 0),
    ...groups.map((g) => ({ ...g.row, tags: [...g.row.tags], sig: g.sig, id: `p:${g.id}`, group: g, quantity: g.qty }))];
}

/** Moves one copy of `r` into a group of its own; again, another into that group. */
export function split(c: Card, r: Row) {
  if (r.quantity < 2) return;
  const groups = pending.get(c.key) || [];
  const child = groups.find((g) => g.id === splitInto.get(r.id) && g.qty > 0);
  if (child) {
    child.qty++;
  } else {
    const { id: _id, quantity: _q, group: _g, sig: _s, ...fields } = r;
    const fresh: Group = { id: nextGroup++, sig: r.sig, row: { ...fields, tags: [...r.tags] }, qty: 1 };
    groups.push(fresh);
    pending.set(c.key, groups);
    splitInto.set(r.id, fresh.id);
  }
  if (r.group) r.group.qty--;
  splits.version++;
}

/** The purchases to save for `c`: every row, with `changed` (row ID → new fields)
 *  applied. Split groups that didn't change merge back into theirs on the server, and
 *  stay split here. */
function purchasesOf(c: Card, changed: Record<string, Partial<Fields>>): Fields[] {
  return rowsOf(c).map((r) => ({
    finish: r.finish, condition: r.condition, quantity: r.quantity, price_paid: r.price_paid || 0,
    acquired_date: r.acquired_date || "", lot: r.lot || "", tags: r.tags, ...(changed[r.id] || {}),
  })).filter((p) => p.quantity > 0);
}

/** Saves card `c` with `changed` applied and answers the card as saved (null when it
 *  has no copies left and was deleted, or when the user kept it). */
async function save(c: Card, changed: Record<string, Partial<Fields>>): Promise<Card | null | undefined> {
  const before = rowsOf(c);
  const purchases = purchasesOf(c, changed);
  const path = gameAPI(`/card?key=${encodeURIComponent(c.key)}`);
  if (!purchases.length) {
    if (!(await ask(`Remove ${c.name} from your collection? It has no copies left.`, "Remove"))) return undefined;
    await api("DELETE", path);
  } else {
    await api("PUT", path, { purchases });
  }
  // Changed rows keep their place under their new signature; saved groups are done.
  const seen = order.get(c.key) || [];
  for (const [id, fields] of Object.entries(changed)) {
    const row = before.find((r) => r.id === id);
    if (!row) continue;
    const now = sig({ ...row, ...fields });
    const at = seen.indexOf(row.sig);
    if (id.startsWith("s:") && at >= 0) seen[at] = now; else if (!seen.includes(now)) seen.push(now);
    if (row.group) pending.set(c.key, (pending.get(c.key) || []).filter((g) => g !== row.group));
  }
  splits.version++;
  if (!purchases.length) return null;
  return (await api<{ card: Card }>("GET", path)).card;
}

/** What a card's rows in the table can do. */
export interface RowActions {
  /** Change `field` on `row`, or on every row shown (row null). `value` may be a
   *  function of each row's old value. */
  edit: (row: Row | null, field: keyof Fields, value: unknown, label: string) => void;
  split: (row: Row) => void;
  /** Make `row`'s copies like `into`'s (finish, condition, price, date, lot, tags), so
   *  the two groups become one. */
  merge: (row: Row, into: Row) => void;
  sell: (row: Row) => void;
  remove: (row: Row) => void;
  filter: (name: "tags" | "lot", value: string) => void;
  open: () => void;
}

/** One card's edit: what to change on which of its rows. */
export interface Edit { card: Card; changed: Record<string, Partial<Fields>> }

/** Saves `edits` (one card each), then calls `saved(old, now)` for each card so the
 *  table can show it. The status line says `message`, with Undo putting every card's
 *  purchases back as they were. */
export async function saveEdits(edits: Edit[], message: string, saved: (old: Card, now: Card | null) => void) {
  const undo: { card: Card; purchases: Fields[] }[] = [];
  // Moving copies into or out of a lot, or changing how many a lot holds, reprices
  // the lot's other cards too: the list reloads to show their new prices.
  const repriced = edits.some((e) => Object.entries(e.changed).some(([id, f]) =>
    "lot" in f || ("quantity" in f && rowsOf(e.card).find((r) => r.id === id)?.lot)));
  try {
    let n = 0;
    for (const e of edits) {
      if (edits.length > 3) status(`Saving ${++n} of ${edits.length}…`);
      const was = purchasesOf(e.card, {});
      const now = await save(e.card, e.changed);
      if (now === undefined) continue;
      undo.push({ card: e.card, purchases: was });
      saved(e.card, now);
    }
    if (repriced) { await loadLots(); reload(); }
    if (edits.some((e) => Object.values(e.changed).some((f) => "tags" in f))) await loadFilters();
    if (!undo.length) { status(""); return; }
    status(message, false, async () => {
      for (const u of undo) {
        await api("PUT", gameAPI(`/card?key=${encodeURIComponent(u.card.key)}`), { purchases: u.purchases });
        pending.delete(u.card.key);
      }
      splits.version++;
      await loadLots();
      await loadFilters();
    });
  } catch (e) {
    status(e instanceof Error ? e.message : String(e), true);
  }
}

/** "3 copies of Shock", or "12 copies of 4 cards". */
export function said(edits: Edit[], copies: number) {
  return edits.length === 1 ? `${plural(copies, "copy", "copies")} of ${edits[0].card.name}` : `${plural(copies, "copy", "copies")} of ${edits.length} cards`;
}

/** Forgets a card's split groups (after it's deleted). */
export function forget(key: string) { pending.delete(key); splits.version++; }
