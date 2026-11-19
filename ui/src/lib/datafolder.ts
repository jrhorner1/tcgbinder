// The data folder: where the collection is kept, and moving it.

import { api, failed, status } from "./app.svelte";
import { askDialog } from "./dialogs.svelte";
import { plural } from "./format";

export interface DataFolder {
  path: string; isDefault: boolean; defaultPath: string; fixed: boolean; firstRun: boolean; canPick: boolean; restartsItself: boolean;
  conflicts: { game: string; name: string; kind: string; cards: number | null }[];
}

/** Where the collection is kept, or null where it can't be changed. */
export const dataFolder = () => api<DataFolder>("GET", "/api/data-folder").catch(() => null);

/** Switches to `path` ("" for the default folder): looks at what's there, asks what
 *  to do, then has the app restart. */
export async function changeDataFolder(df: DataFolder, path: string) {
  const target = path || df.defaultPath;
  let seen: { exists: boolean; writable: boolean; games: { id: string; cards: number }[] };
  try { seen = await api("POST", "/api/data-folder/inspect", { path: target }); } catch (e) { failed(e); return; }
  if (!seen.writable) { status(`TCG Binder can't write to ${target}. Choose another folder.`, true); return; }
  const cards = seen.games.reduce((n, g) => n + g.cards, 0);
  const mode = seen.games.length
    ? await askDialog("Use this folder?", [`This folder has a collection of ${plural(cards, "unique card", "unique cards")}: ${target}`, "Use it? Your current collection stays where it is."], [{ label: "Use it", value: "use", primary: true }])
    : await askDialog("Move your collection?", [`Move your collection to ${target}, or start fresh there?`, "Moving takes your cards, price history and backups along."], [{ label: "Start fresh", value: "use" }, { label: "Move it here", value: "move", primary: true }]);
  if (!mode) return;
  try {
    const r = await api("POST", "/api/data-folder", { path, mode });
    if (r.restartsItself) status(`Using ${r.path}. Restarting…`);
    else await askDialog("Restart TCG Binder", [`Your collection is now kept in ${r.path}.`, "Stop TCG Binder and start it again to use it. Until then, changes can't be saved."], [{ label: "OK", value: true, primary: true }], false);
  } catch (e) { failed(e); }
}

/** Shows the system's folder picker, then switches to the folder. */
export async function chooseDataFolder(df: DataFolder) {
  try {
    const r = await api("POST", "/api/data-folder/pick");
    if (r?.path) await changeDataFolder(df, r.path);
  } catch (e) { failed(e); }
}
