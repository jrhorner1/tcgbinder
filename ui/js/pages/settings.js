// Settings, and the data folder: where the collection is kept, moving it, and the
// notices about it shown on Home.

import { api, dataFolder, gameAPI, loadFilters, loadLots, saveSettings } from "../api.js";
import { $, askDialog, confirmDialog, fill, h, status, store } from "../dom.js";
import { choiceNames, plural } from "../format.js";
import { go, route } from "../nav.js";
import { state } from "../state.js";

// changeDataFolder switches to `path` ("" for the default folder): it looks at what is
// there, asks what to do, then has the app restart.
async function changeDataFolder(df, path) {
  const target = path || df.defaultPath;
  let seen;
  try { seen = await api("POST", "/api/data-folder/inspect", { path: target }); } catch (e) { status(e.message, true); return; }
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
  } catch (e) { status(e.message, true); }
}

// chooseDataFolder shows the system's folder picker, then switches to the folder.
async function chooseDataFolder(df) {
  try {
    const r = await api("POST", "/api/data-folder/pick");
    if (r.path) await changeDataFolder(df, r.path);
  } catch (e) { status(e.message, true); }
}

// dataSection is Settings → Data: where the collection is kept, and changing it.
function dataSection(df, fallback) {
  const intro = h("p.muted", "Your collections, price history and backups are kept in this folder. Choose a folder in iCloud Drive, Dropbox or OneDrive to share them between devices. Settings stay on each device.");
  if (!df) return [intro, h("p.settings-path", h("code", fallback || "(not saved)"))];
  const where = h("p.settings-path", h("code", df.path), df.isDefault ? h("span.muted", " (the default)") : null);
  if (df.fixed) return [intro, where, h("p.muted", "Set by --data-dir or TCGBINDER_DATA_DIR when TCG Binder was started.")];
  if (state.info.client === "phone") return [intro, where];
  const useDefault = df.isDefault ? null : h("button", { type: "button", onclick: () => changeDataFolder(df, "") }, "Use the default folder");
  if (df.canPick) return [intro, where, h("div.buttons", h("div.right", h("button", { type: "button", onclick: () => chooseDataFolder(df) }, "Change…"), useDefault))];
  const field = h("input", { type: "text", placeholder: "/full/path/to/a/folder", "aria-label": "Data folder path" });
  return [intro, where, h("div.search", field, h("button", { type: "button", onclick: () => field.value.trim() && changeDataFolder(df, field.value.trim()) }, "Use this folder")),
    h("p.muted", "A folder on the computer running TCG Binder."), useDefault ? h("p", useDefault) : null];
}

// dataNotices are Home's notices about the data folder: the first-run welcome and
// conflict copies another device left.
export function dataNotices(df, home) {
  if (!df) return [];
  const out = [];
  if (df.firstRun && !home.cards && !store("welcomed")) {
    const panel = h("div.card-box.notice",
      h("h2", "Welcome to TCG Binder"),
      h("p", `Your collection will be kept in ${df.path}. Keep it there, or choose a folder (for example in iCloud Drive or Dropbox to share it between devices).`),
      h("div.buttons", h("div.right",
        h("button", { type: "button", onclick: () => { store("welcomed", "1"); panel.remove(); } }, "Keep it there"),
        df.canPick ? h("button.primary", { type: "button", onclick: () => chooseDataFolder(df) }, "Choose a folder…")
          : h("a.button", { href: "#/settings" }, "Choose a folder…"))));
    out.push(panel);
  }
  for (const c of df.conflicts.filter((c) => c.game === state.game)) {
    const act = async (what) => {
      if (what === "discard" && !(await confirmDialog(`Delete ${c.name}? Its changes are lost.`, { danger: true, action: "Discard it" }))) return;
      try {
        const r = await api("POST", gameAPI(`/conflicts/${what}`), { name: c.name });
        const unit = { lots: ["lot", "lots"], sales: ["sale", "sales"], lists: ["list", "lists"] }[c.kind] || ["card", "cards"];
        status(what === "merge" ? `Merged ${c.name}: ${plural(r.changed, ...unit)} changed.` : `Discarded ${c.name}.`, false, r.undo);
        await loadFilters();
        route();
      } catch (e) { status(e.message, true); }
    };
    const about = {
      lots: ["your lots", "Merging adds the lots only it has; where both have a lot, the newer one is kept."],
      sales: ["your sales", "Merging adds the sales only it has; none is counted twice."],
      lists: ["your lists", "Merging adds the lists and cards only it has; where both have a card on a list, the larger quantity is kept."],
    }[c.kind] || ["your collection", "Merging adds its cards and copies to yours without counting any twice; a card deleted on one device comes back."];
    out.push(h("div.card-box.notice",
      h("h2", `Another device saved a different version of ${about[0]}`),
      h("p", `${c.name}${c.kind === "collection" && c.cards !== null ? ` (${plural(c.cards, "unique card", "unique cards")})` : ""} was saved by another device at the same time as this one. ${about[1]} Discarding deletes that file.`),
      h("div.buttons", h("div.right",
        h("button", { type: "button", onclick: () => act("discard") }, "Discard it"),
        h("button.primary", { type: "button", onclick: () => act("merge") }, "Merge it")))));
  }
  return out;
}

// deleteAllData asks before deleting every game's collection, lots, sales, lists,
// watchlist, sealed product and price history: the word "delete" must be typed, and a
// backup of the collection is kept unless unticked.
function deleteAllData() {
  const dlg = $("#dialog");
  const word = h("input", { type: "text", placeholder: "delete", "aria-label": "Type delete to confirm", autocomplete: "off" });
  const keep = h("input", { type: "checkbox", checked: true });
  const err = h("p.error");
  const go1 = h("button.danger", { type: "button", disabled: true, onclick: async () => {
    err.textContent = "";
    go1.disabled = true;
    try {
      const r = await api("POST", "/api/data/delete-all", { confirm: word.value, keepBackup: keep.checked });
      dlg.close();
      state.ticked.clear();
      state.addDefaults = null;
      await loadFilters();
      await loadLots();
      status(Object.keys(r.backups).length ? "Deleted all data. The collection's backup is under Tools → Restore a backup." : "Deleted all data.");
      go("home");
    } catch (e) { err.textContent = e.message; go1.disabled = false; }
  } }, "Delete all data");
  word.addEventListener("input", () => { go1.disabled = word.value.trim().toLowerCase() !== "delete"; });
  fill(dlg, h("div.dlg", h("header", h("h2", "Delete all data?")),
    h("div.body",
      h("p", "This deletes every card in your collection, your lots, sales, lists, watchlist, sealed product and price history, for every game. Your settings stay."),
      h("p", h("strong", "It can't be undone"), " unless you keep a backup of the collection below. Lots, sales, lists, the watchlist and sealed product are not in the backup."),
      h("label.check", keep, "Keep a backup of the collection (restore it from Tools → Restore a backup)"),
      h("p", "Type ", h("strong", "delete"), " to confirm:"), word, err),
    h("footer", h("button", { type: "button", onclick: () => dlg.close() }, "Cancel"), go1)));
  dlg.showModal();
  word.focus();
}

export async function viewSettings() {
  const d = await api("GET", "/api/settings");
  state.settings = d.settings;
  const s = d.settings;
  const menu = (key, what) => h("select", { id: `set-${key}`, onchange: (e) => saveSettings({ [key]: e.target.value }, what) },
    d.choices[key].map((v) => h("option", { value: v, selected: v === s[key] }, (choiceNames[key] || {})[v] || v)));
  const check = (key, label, what) => h("label.check", h("input", { type: "checkbox", id: `set-${key}`, checked: s[key],
    onchange: (e) => saveSettings({ [key]: e.target.checked }, what) }), label);
  const row = (label, control, hint) => [h("label", { for: control.id || null }, label), h("div", control, hint ? h("div.hint", hint) : null)];
  const section = (name, ...rows) => [h("h2", name), h("div.card-box", h("div.form.settings", { style: "margin:0" }, rows.flat()))];
  const phone = state.info.client === "phone";
  fill($("#main"),
    h("h1", "Settings"),
    h("p.muted", "Changes are saved right away and apply to every game."),
    section("Appearance",
      row("Theme", menu("theme", "theme"), "Light, dark, or follow the system setting"),
      row("Text size", menu("text_size", "text size"), "Scales text and spacing"),
      row("Card images", check("show_images", "Show card images", "card images"), "Turn off to save data on slow or metered connections")),
    section("Collection",
      row("Card order", menu("card_order", "card order"), "How cards are listed in the collection, until you choose a Sort there"),
      row("Set completion", menu("set_completion", "set completion"), "What a set's completion counts: the cards numbered up to the size printed on them, or every card including variants, promos and extras"),
      row("Value chart", menu("chart_range", "chart range"), "The range the Value Over Time chart shows first")),
    section("Prices",
      row("Update prices", menu("price_refresh", "price updates"), "You can also refresh them on the Tools page"),
      row("Price history", check("auto_price_history", "Load price history automatically", "price history"), "The first time, on desktop and web (about a 50 MB download); phones ask first")),
    section("Adding Cards",
      row("Condition", menu("default_condition", "default condition"), "Preselected when adding a card"),
      row("Acquired date", menu("default_date", "default date"), "Filled in when adding a card")),
    section("Advanced",
      row("Deleting", check("confirm_delete", "Ask before deleting cards", "delete confirmation")),
      row("Log detail", menu("log_level", "log level"), phone ? "How much goes in the log file" : "How much goes in the log file; --log-level overrides it at startup")),
    h("h2", "Data"),
    dataSection(await dataFolder(), d.dataDir),
    h("div.card-box.danger-zone", { style: "margin-top:12px" },
      h("div.tool", h("button.danger", { type: "button", onclick: deleteAllData }, "Delete all data…"),
        h("p.muted", "Delete your whole collection and everything with it (lots, sales, lists, watchlist, sealed product, price history), for every game. You'll be asked to confirm."))),
    h("h2", "About"),
    h("p", `${state.info.name} ${state.info.version}`),
    h("p", h("button", { type: "button", onclick: async () => {
      if (!(await confirmDialog("Put every setting back to how it was when the app was installed? Your collection isn't affected.", { action: "Restore defaults" }))) return;
      await saveSettings(d.defaults, "defaults restored");
      route();
    } }, "Restore defaults")));
}
