// Making a list of a set's missing cards (from the set's page and the Missing page).

import { api, failed, gameAPI, status } from "./app.svelte";
import { askName } from "./dialogs.svelte";
import { plural } from "./format";
import { go } from "./router.svelte";
import type { CatalogCard } from "./types";

export interface Missing { missing: CatalogCard[]; total: number; printedSize: number | null; totalCost: number; unpriced: number }

/** Asks for a name ("Core Set 2019 Missing" to start with), puts one of each card of
 *  set `code` you don't own on a new list, and opens it. The list takes cards off as
 *  they join the collection. `m` is the missing cards when they're already loaded. */
export async function createMissingList(code: string, setName: string, m?: Missing) {
  const name = await askName("Create a list of the missing cards", `${setName || code.toUpperCase()} Missing`);
  if (!name) return;
  try {
    status("Loading the set's cards…");
    const missing = m || (await api<Missing>("GET", gameAPI(`/missing?set=${encodeURIComponent(code)}`)));
    if (!missing.missing.length) { status("You have every card in this set, so there's nothing to put on a list."); return; }
    const list = await api("POST", gameAPI("/lists"), { name, remove_owned: true });
    await api("PUT", gameAPI(`/lists?id=${encodeURIComponent(list.id)}`),
      { entries: missing.missing.map((c) => ({ card: c, quantity: 1, any_printing: false, finish: "" })) });
    status(`Made the list ${name} with ${plural(missing.missing.length, "card", "cards")}.`);
    go("lists", { id: list.id });
  } catch (e) { failed(e); }
}
