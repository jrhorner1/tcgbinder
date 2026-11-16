// TCG Binder's user interface, shared by the desktop app, the iPhone app and the web
// app (--web). Plain JavaScript modules, no build step. This file starts the app: it
// loads the info and settings, draws the page for the address (route), and watches for
// changes made in the background. It is the only file that imports the pages.

import { api, loadFilters } from "./api.js";
import { $, applySettings, fill, h, status, store } from "./dom.js";
import { gameInfo, shortName } from "./format.js";
import { go, setRoute } from "./nav.js";
import { state } from "./state.js";
import { viewAdd } from "./pages/add.js";
import { viewCollection } from "./pages/collection.js";
import { viewEarnings } from "./pages/earnings.js";
import { viewHome } from "./pages/home.js";
import { viewLists } from "./pages/lists.js";
import { viewLots } from "./pages/lots.js";
import { viewScan } from "./pages/scan.js";
import { viewSealed } from "./pages/sealed.js";
import { viewSearch } from "./pages/search.js";
import { viewSettings } from "./pages/settings.js";
import { viewStats } from "./pages/stats.js";
import { viewTags } from "./pages/tags.js";
import { viewBackups, viewImport, viewTools } from "./pages/tools.js";
import { viewWatchlist } from "./pages/watchlist.js";

// addMenu makes the sidebar's Add cards button a menu: a single card, or a pack,
// bundle or lot.
function addMenu() {
  const button = $("#add-menu-button"), items = $("#add-menu-items");
  if (!button || !items) return;
  const close = () => { items.hidden = true; button.setAttribute("aria-expanded", "false"); };
  button.addEventListener("click", () => {
    items.hidden = !items.hidden;
    button.setAttribute("aria-expanded", String(!items.hidden));
    if (!items.hidden) items.querySelector("a").focus();
  });
  items.addEventListener("click", close);
  document.addEventListener("click", (e) => { if (!e.target.closest(".add-menu")) close(); });
  document.addEventListener("keydown", (e) => { if (e.key === "Escape" && !items.hidden) { close(); button.focus(); } });
}

export async function route() {
  const [path, qs] = location.hash.replace(/^#\/?/, "").split("?");
  const view = path || "home";
  if (state.scanStop && view !== "scan") { state.scanStop(); state.scanStop = null; }
  const active = ["import", "backups", "earnings", "watchlist", "sealed", "tags"].includes(view) ? "tools" : view === "scan" ? "add" : view;
  document.querySelectorAll("#nav a").forEach((a) => a.classList.toggle("active", a.dataset.view === active));
  // The tab bar has no Lists, Statistics or Lots tab: they're reached from Tools there.
  document.querySelectorAll(".tabbar a").forEach((a) => a.classList.toggle("active", a.dataset.view === (["lists", "stats", "lots"].includes(active) ? "tools" : active)));
  try {
    if (view === "collection") await viewCollection(qs || "");
    else if (view === "add") viewAdd(qs || "");
    else if (view === "scan") viewScan();
    else if (view === "search") await viewSearch(qs || "");
    else if (view === "import") viewImport();
    else if (view === "backups") await viewBackups();
    else if (view === "lots") await viewLots(qs || "");
    else if (view === "earnings") await viewEarnings(qs || "");
    else if (view === "lists") await viewLists(qs || "");
    else if (view === "watchlist") await viewWatchlist();
    else if (view === "sealed") await viewSealed();
    else if (view === "stats") await viewStats();
    else if (view === "tags") await viewTags();
    else if (view === "tools") await viewTools();
    else if (view === "settings") await viewSettings();
    else await viewHome();
  } catch (e) {
    fill($("#main"), h("p.error", e.message));
  }
}

// Background work (the daily price refresh, a load in another window) changes the
// data; the open page refreshes unless the user is in the middle of something.
async function watchForChanges() {
  try {
    const { revision } = await api("GET", "/api/status");
    const busy = document.activeElement && ["INPUT", "SELECT", "TEXTAREA"].includes(document.activeElement.tagName);
    if (state.skipRevision || state.revision === null) {
      state.revision = revision;
      state.skipRevision = false;
    } else if (revision !== state.revision && !busy && !document.querySelector("dialog[open]") && !location.hash.startsWith("#/scan") && !location.hash.startsWith("#/add")) {
      state.revision = revision;
      await loadFilters();
      route();
    }
  } catch { /* the app may be closing */ }
}

function showGame() {
  const g = gameInfo();
  document.querySelectorAll(".game-name").forEach((el) => { el.textContent = shortName(g.Name); });
  document.querySelectorAll("[data-scan]").forEach((el) => { el.hidden = !g.Scan; });
}

async function init() {
  state.info = await api("GET", "/api/info");
  state.settings = (await api("GET", "/api/settings")).settings;
  applySettings();
  state.range = state.settings.chart_range;
  document.documentElement.dataset.client = state.info.client;
  const saved = store("game");
  state.game = state.info.games.some((g) => g.ID === saved) ? saved : state.info.games[0].ID;
  const sel = $("#game");
  fill(sel, ...state.info.games.map((g) => h("option", { value: g.ID, selected: g.ID === state.game }, g.Name)));
  sel.hidden = state.info.games.length < 2;
  sel.addEventListener("change", async () => { state.game = sel.value; store("game", sel.value); showGame(); await loadFilters(); go("home"); });
  $("#globalsearch").addEventListener("submit", (e) => { e.preventDefault(); const q = $("#gsq").value.trim(); if (q) go("search", new URLSearchParams({ q })); });
  $("#version").textContent = `${state.info.name} ${state.info.version}`;
  document.title = state.info.name;
  showGame();
  await loadFilters();
  window.addEventListener("hashchange", route);
  addMenu();
  // In the apps, the web view drops links that open a new window (View on Scryfall):
  // links to other sites open in the system's browser instead.
  document.addEventListener("click", (e) => {
    const a = e.target.closest && e.target.closest("a[href]");
    if (!a || !state.info.opensLinks || !/^https?:/i.test(a.href) || new URL(a.href).origin === location.origin) return;
    e.preventDefault();
    api("POST", "/api/open", { url: a.href }).catch((err) => status(err.message, true));
  });
  setInterval(() => { if (!document.hidden) watchForChanges(); }, 15000);
  document.addEventListener("visibilitychange", () => { if (!document.hidden) watchForChanges(); });
  route();
}

setRoute(route);
init().catch((e) => fill($("#main"), h("p.error", `Could not start: ${e.message}`)));
