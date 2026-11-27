// What a card's row shows in a copy column: the value all its rows share, or MIXED.

import type { Fields, Row } from "./edits.svelte";

export const MIXED = Symbol("mixed");
export type Shown<T> = T | typeof MIXED;

/** The value every row has for `field`, or MIXED when they differ. */
export function common<K extends keyof Fields>(rows: Row[], field: K): Shown<Fields[K]> {
  if (!rows.length) return MIXED;
  const key = (r: Row) => JSON.stringify(field === "tags" ? [...r.tags].map((t) => t.toLowerCase()).sort() : r[field]);
  const first = key(rows[0]);
  return rows.every((r) => key(r) === first) ? rows[0][field] : MIXED;
}
