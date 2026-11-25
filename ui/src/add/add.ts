// Opening the Add dialog, from the sidebar, the phone's Add tab, the search at the top
// and an unfinished lot's banner. When it closes having added something, the
// collection shows the result.

import { app, loadLots, reload } from "../lib/app.svelte";
import { openDialog } from "../lib/dialogs.svelte";
import { go, route } from "../lib/router.svelte";
import type { CatalogCard } from "../lib/types";
import AddDialog from "./AddDialog.svelte";

/** Opens the Add dialog: adding a card, or entering the cards of the open lot `lot`
 *  (its ID). `query` (with `set`, a set code) searches for a card straight away;
 *  `card` offers that one printing. */
export async function openAdd({ lot = "", query = "", set = "", card = null as CatalogCard | null } = {}) {
  const changed = await openDialog<boolean>(AddDialog, { lot: app.lots.find((l) => l.id === lot) || null, query: card ? card.name : query, set, card });
  if (!changed) return;
  await loadLots();
  if (route.view === "collection") reload(); else go("collection");
}
