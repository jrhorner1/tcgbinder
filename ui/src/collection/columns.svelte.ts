// The collection table's columns: which are shown (Columns… changes it, and the choice
// is kept), what each sorts by, and which a phone has room for.

import { store } from "../lib/app.svelte";

export interface Column {
  key: "set" | "qty" | "finish" | "condition" | "language" | "lot" | "paid" | "acquired" | "tags" | "value";
  label: string;
  /** GET /cards's sort for it, when clicking its heading sorts. */
  sort?: string;
  /** Always shown (not in Columns…). */
  always?: boolean;
  /** Shown on a phone too (the others need a wider screen). */
  phone?: boolean;
  on: boolean;
}

const DEFAULTS: Column[] = [
  { key: "set", label: "Set", sort: "set", on: true },
  { key: "qty", label: "Qty", sort: "quantity", always: true, phone: true, on: true },
  { key: "finish", label: "Finish", on: true },
  { key: "condition", label: "Condition", on: true },
  { key: "language", label: "Language", on: false },
  { key: "lot", label: "Purchase", on: true },
  { key: "paid", label: "Paid each", sort: "paid", on: true },
  { key: "acquired", label: "Acquired", sort: "acquired", on: true },
  { key: "tags", label: "Tags", sort: "tag", on: true },
  { key: "value", label: "Value", sort: "value", always: true, phone: true, on: true },
];

const saved = (() => { try { return JSON.parse(store("columns") || "{}") as Record<string, boolean>; } catch { return {}; } })();

export const columns = $state<Column[]>(DEFAULTS.map((c) => ({ ...c, on: c.always || (saved[c.key] ?? c.on) })));

export const shown = (key: Column["key"]) => columns.find((c) => c.key === key)!.on;

export function toggle(key: Column["key"]) {
  const c = columns.find((x) => x.key === key)!;
  c.on = !c.on;
  store("columns", JSON.stringify(Object.fromEntries(columns.map((x) => [x.key, x.on]))));
}
