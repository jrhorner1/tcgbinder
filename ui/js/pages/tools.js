// Tools: importing (with its preview), exporting, backups and the other tools.

import { api, exportCollection, gameAPI, loadFilters, runJob } from "../api.js";
import { tagEditor } from "../cards.js";
import { tile } from "../charts.js";
import { $, askDialog, confirmDialog, fill, h, status, toolRow, whileRunning } from "../dom.js";
import { plural, usd } from "../format.js";
import { state } from "../state.js";

// Import: a CSV export from another app, or a collection .json from this app. The
// file is previewed first (matched, nothing saved), then imported on request.
export function viewImport() {
  const file = h("input", { type: "file", accept: ".csv,.json,text/csv,application/json" });
  const out = h("div");
  const previewBtn = h("button.primary", { type: "button", onclick: async () => {
    if (!file.files.length) { status("Choose a file first."); return; }
    previewBtn.disabled = true;
    fill(out);
    status(`Reading ${file.files[0].name}… matching cards can take a minute for large files.`);
    try {
      const p = await whileRunning(api("POST", gameAPI("/import?preview=1"), file.files[0]));
      status("");
      showImportPreview(out, p, file.files[0].name, () => { file.value = ""; fill(out); });
    } catch (e) { status(e.message, true); }
    previewBtn.disabled = false;
  } }, "Preview");
  fill($("#main"),
    h("h1", "Import"),
    h("p", "Import a CSV export from another collection app (EchoMTG, ManaBox, Moxfield, Deckbox, TCGplayer, Dragon Shield, Archidekt and others), or a collection .json file exported from this app. Cards you already own add to their quantities."),
    h("p.muted", "You'll see what the file holds before anything is saved."),
    h("div.card-box", h("div.search", file, previewBtn)), out);
}

// unmatchedLink offers the rows that matched no card as a CSV with the file's header,
// to fix and import again.
function unmatchedLink(p, fileName) {
  const text = [p.header, ...p.unmatched.map((u) => u.text)].filter(Boolean).join("\n") + "\n";
  const url = URL.createObjectURL(new Blob([text], { type: "text/csv" }));
  const name = fileName.replace(/\.[^.]*$/, "") + "-unmatched.csv";
  return h("a.button", { href: url, download: name }, "Download the unmatched rows");
}

// showImportPreview shows what importing would do, with Import and Cancel.
export function showImportPreview(out, p, fileName, cancel, lot, done) {
  const importTags = tagEditor([], { label: "Tag every card in this import" });
  const importBtn = h("button.primary", { type: "button", onclick: async () => {
    importBtn.disabled = true;
    status(`Importing ${plural(p.copies, "card", "cards")}…`);
    try {
      let tags;
      try { tags = importTags.value(); } catch (e) { status(e.message, true); importBtn.disabled = false; return; }
      const body = { token: p.token, tags };
      if (lot) body.lot = lot.id;
      const r = await whileRunning(api("POST", gameAPI("/import/commit"), body));
      status(`Imported ${plural(r.cards, "card", "cards")} from ${r.format}${lot ? ` into ${lot.name}` : ""}.`, false, r.undo);
      if (done) done(r);
      fill(out, h("h2", "Import Complete"), h("pre.summary", r.summary));
      await loadFilters();
    } catch (e) { status(e.message, true); importBtn.disabled = false; }
  } }, `Import ${plural(p.copies, "card", "cards")}`);
  const unmatched = p.unmatched.length;
  fill(out,
    h("h2", `Preview: ${fileName}`),
    h("p.muted", `${p.format} · ${plural(p.rows, "row", "rows")} read. Nothing has been saved yet.`),
    h("div.tiles",
      tile("CARDS", p.copies.toLocaleString(), plural(p.cards, "printing", "printings")),
      tile("NEW", p.newCards.toLocaleString(), "cards you don't own yet"),
      tile("ALREADY OWNED", p.alreadyOwned.toLocaleString(), "add to cards you own"),
      tile("PRICE PAID", usd(p.pricePaid), "for cards with a price")),
    unmatched ? h("p", h("strong", `${plural(unmatched, "row", "rows")} matched no card and won't be imported.`),
      " Download them, fix them, and import them again.") : null,
    unmatched ? h("p", unmatchedLink(p, fileName)) : null,
    unmatched ? h("details", h("summary", "Show the unmatched rows"),
      h("pre.raw", p.unmatched.map((u) => `Line ${u.line}: ${u.text}`).join("\n"))) : null,
    p.notes.length ? h("pre.summary", p.notes.join("\n")) : null,
    p.copies ? h("div.import-tags", h("label", "Tag every card in this import (optional)"), importTags.el) : null,
    h("div.buttons", h("div.right", p.copies ? importBtn : null, h("button", { type: "button", onclick: cancel }, "Cancel"))));
}

// Backups: the collection as it was before each import, bulk change and restore.
export async function viewBackups() {
  const list = await api("GET", gameAPI("/backups"));
  const why = { import: "Before an import", "bulk-edit": "Before a bulk edit", "bulk-delete": "Before a bulk delete", restore: "Before a restore", merge: "Before a merge", move: "Before a move",
    "tag-rename": "Before renaming a tag", "tag-delete": "Before deleting a tag", "delete-all": "Before deleting all data" };
  const restore = async (b, button) => {
    if (!(await confirmDialog(`Replace your collection with the backup from ${b.date} (${plural(b.cards, "unique card", "unique cards")})? Your current collection is backed up first, so you can come back to it.`, { action: "Restore" }))) return;
    button.disabled = true;
    try {
      const r = await api("POST", gameAPI("/backups/restore"), { name: b.name });
      status(`Restored the backup from ${b.date}: ${plural(r.cards, "unique card", "unique cards")}.`, false, r.undo);
      await loadFilters();
      await viewBackups();
    } catch (e) { status(e.message, true); button.disabled = false; }
  };
  fill($("#main"),
    h("h1", "Restore a Backup"),
    h("p", "A copy of the collection is saved before every import, bulk edit, bulk delete and restore. The last 10 are kept."),
    list.length ? h("div.card-box", list.map((b) => {
      const button = h("button", { type: "button", onclick: () => restore(b, button) }, "Restore");
      return toolRow(button, `${b.date} · ${why[b.reason] || b.reason} · ${plural(b.cards, "unique card", "unique cards")}`);
    })) : h("p.muted", "No backups yet. One is made before your first import or bulk change."));
}

// exportAll asks which kind of file, then saves the whole collection as it.
async function exportAll(button) {
  const format = await askDialog("Export the collection", [
    "A collection file keeps everything exactly, for a backup or to move to another device. CSV opens in spreadsheets and other collection apps.",
    "Import reads either back.",
  ], [{ label: "CSV", value: "csv" }, { label: "Collection file", value: "json", primary: true }]);
  if (format) exportCollection(button, format);
}

export async function viewTools() {
  const o = await api("GET", gameAPI("/home"));
  const s = state.settings;
  const every = { daily: "automatically each day", weekly: "automatically each week", manual: "only when you ask" }[s.price_refresh];
  const job = (label, path, busy, done) => h("button", { type: "button", onclick: (e) => runJob(e.currentTarget, path, busy, done) }, label);
  const where = state.info.client === "phone" ? "the app's folder in the Files app" : state.info.saveExports ? "your Downloads folder" : "your downloads";
  fill($("#main"),
    h("h1", "Tools"),
    state.info.games.length > 1 ? h("p.muted", `For ${o.name}. Choose a game in the sidebar to switch.`) : null,
    h("h2", "Collection"),
    h("div.card-box",
      toolRow(h("a.button", { href: "#/import" }, "Import…"), "Add cards from another app's CSV export (EchoMTG, ManaBox, Moxfield, Deckbox, TCGplayer and others) or from a collection file exported here."),
      toolRow(h("button", { type: "button", onclick: (e) => exportAll(e.currentTarget) }, "Export…"), `Save the collection to ${where}: as a collection file, for a backup or to move to another device, or as a CSV file for spreadsheets and other collection apps (a row per purchase, with its price paid, date, tags and market price). Import reads either back. To export only some cards, filter the collection list and choose Export there.`),
      toolRow(h("a.button", { href: "#/tags" }, "Tags…"), "Every tag on your cards, with how many cards and copies carry it and what they're worth. Rename or delete a tag everywhere at once."),
      toolRow(h("a.button", { href: "#/stats" }, "Statistics…"), "Your collection broken down by rarity, finish, condition, language, release year, color and card type, by price band, and by month acquired."),
      toolRow(h("a.button", { href: "#/sealed" }, "Sealed product…"), "Boxes, bundles and decks you keep unopened: what you paid and what they're worth. There's no free price source for sealed product, so you enter each value yourself."),
      toolRow(h("a.button", { href: "#/watchlist" }, "Watchlist…"), "Cards whose price you're watching, with an alert when one moves by a percentage or crosses a price. Prices are checked when the app refreshes them, so alerts appear while the app is open or the next time you open it."),
      toolRow(h("a.button", { href: "#/backups" }, "Restore a backup…"), "Go back to the collection as it was before a recent import, bulk edit, bulk delete or deleting all data. The last 10 backups are kept."),
    ),
    h("h2", "Prices"),
    h("div.card-box",
      toolRow(job("Refresh prices now", "/refresh", "Refreshing prices…", (r) => `Prices updated for ${plural(r.updated, "card", "cards")}`),
        `Fetch today's market prices now. ${o.pricesAsOf ? `Prices are from ${o.pricesAsOf}` : "No prices have been fetched yet"}; they update ${every} (see Settings).`),
      o.canLoadHistory ? toolRow(job(o.historyLoaded ? "Update price history" : "Load price history", "/price-history", "Loading price history… this downloads about 50 MB.", (r) => `Loaded price history for ${plural(r.printings, "printing", "printings")}`),
        o.historyLoaded ? `Price history last loaded ${o.historyLoaded}. Download ${o.historyInfo} again to fill in recent days.`
          : `Download ${o.historyInfo} so the value chart shows the past, not just the days the app was running.`) : null));
}
