<!-- The app: loads the info and settings, draws the shell (sidebar, search, tab bar)
     and the page the address names, and refreshes when the data changes elsewhere. -->
<script lang="ts">
  import { onMount, type Component } from "svelte";
  import { api, app, applySettings, loadFilters, loadLots, reload, store, undoChange } from "./lib/app.svelte";
  import { dialogs } from "./lib/dialogs.svelte";
  import { go, route } from "./lib/router.svelte";
  import type { Info, Settings } from "./lib/types";
  import { openAdd } from "./add/add";
  import { gameInfo } from "./lib/cards";
  import GlobalSearch from "./components/GlobalSearch.svelte";
  import Icon, { type IconName } from "./components/Icon.svelte";
  import Backups from "./pages/Backups.svelte";
  import Collection from "./pages/Collection.svelte";
  import Home from "./pages/Home.svelte";
  import Import from "./pages/Import.svelte";
  import Lists from "./pages/Lists.svelte";
  import Missing from "./pages/Missing.svelte";
  import Scan from "./pages/Scan.svelte";
  import Sealed from "./pages/Sealed.svelte";
  import Search from "./pages/Search.svelte";
  import SettingsPage from "./pages/Settings.svelte";
  import Stats from "./pages/Stats.svelte";
  import Tags from "./pages/Tags.svelte";
  import Tools from "./pages/Tools.svelte";
  import Watchlist from "./pages/Watchlist.svelte";

  // Each page is given the address's parameters (most ignore them).
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const PAGES: Record<string, Component<any>> = {
    home: Home, collection: Collection, missing: Missing, search: Search, lists: Lists, tools: Tools, import: Import,
    backups: Backups, tags: Tags, stats: Stats, sealed: Sealed, watchlist: Watchlist, settings: SettingsPage, scan: Scan,
  };
  // Which navigation item is lit for a page that has none of its own.
  const UNDER: Record<string, string> = { import: "tools", backups: "tools", watchlist: "tools", sealed: "tools", tags: "tools", missing: "collection", scan: "collection", search: "collection" };
  const NAV: [string, IconName, string][] = [["home", "home", "Home"], ["collection", "cards", "Collection"], ["stats", "stats", "Statistics"],
    ["lists", "lists", "Lists"], ["tools", "tools", "Tools"], ["settings", "settings", "Settings"]];

  let ready = $state(false);
  let failure = $state("");

  // #/add (the phone's Add tab, old links): the collection, with the Add dialog over it.
  // #/lots?id= (old links): the collection, filtered to the lot.
  $effect(() => {
    if (!ready) return;
    if (route.view === "add") {
      const p = route.params;
      go("collection", {}, true);
      openAdd({ query: p.get("text") || "", set: p.get("set") || "", lot: p.get("lot") || "" });
    } else if (route.view === "lots") {
      const id = route.params.get("id");
      go("collection", id ? { lot: id } : {}, true);
    }
  });
  const view = $derived(route.view in PAGES ? route.view : "home");
  const Page = $derived(PAGES[view]);
  const lit = $derived(UNDER[view] || view);

  // Background work (the daily price refresh, a change on another device) changes the
  // data; the page shown refreshes unless the user is in the middle of something.
  async function watchForChanges() {
    try {
      const { revision } = await api<{ revision: number }>("GET", "/api/status");
      const el = document.activeElement;
      const busy = el && ["INPUT", "SELECT", "TEXTAREA"].includes(el.tagName);
      if (app.skipRevision || app.revision === null) {
        app.revision = revision;
        app.skipRevision = false;
      } else if (revision !== app.revision && !busy && !dialogs.length && route.view !== "scan") {
        app.revision = revision;
        await loadFilters();
        await loadLots();
        reload();
      }
    } catch { /* the app may be closing */ }
  }

  async function switchGame(id: string) {
    app.game = id;
    store("game", id);
    await Promise.all([loadFilters(), loadLots()]);
    go("home");
  }

  onMount(() => {
    (async () => {
      try {
        app.info = await api<Info>("GET", "/api/info");
        app.settings = (await api<{ settings: Settings }>("GET", "/api/settings")).settings;
        applySettings();
        document.documentElement.dataset.client = app.info.client;
        document.title = app.info.name;
        const saved = store("game");
        app.game = app.info.games.some((g) => g.ID === saved) ? saved! : app.info.games[0].ID;
        await Promise.all([loadFilters(), loadLots()]);
        ready = true;
      } catch (e) {
        failure = `Could not start: ${e instanceof Error ? e.message : e}`;
      }
    })();
    // Bars that stick under the top bar (the collection's selection bar) need its
    // height, which differs on phones.
    const sizes = new ResizeObserver(() => {
      const bar = document.querySelector<HTMLElement>(".topbar");
      if (bar) document.documentElement.style.setProperty("--topbar-h", `${bar.offsetHeight}px`);
    });
    const watchBar = setInterval(() => { const bar = document.querySelector(".topbar"); if (bar) { sizes.observe(bar); clearInterval(watchBar); } }, 100);
    const timer = setInterval(() => { if (!document.hidden) watchForChanges(); }, 15000);
    const visible = () => { if (!document.hidden) watchForChanges(); };
    document.addEventListener("visibilitychange", visible);
    return () => { clearInterval(timer); clearInterval(watchBar); sizes.disconnect(); document.removeEventListener("visibilitychange", visible); };
  });

  // In the apps, the web view drops links that open a new window (View on Scryfall):
  // links to other sites open in the system's browser instead.
  function openLinks(e: MouseEvent) {
    const a = (e.target as HTMLElement).closest?.("a[href]") as HTMLAnchorElement | null;
    if (!a || !app.info?.opensLinks || !/^https?:/i.test(a.href) || new URL(a.href).origin === location.origin) return;
    e.preventDefault();
    api("POST", "/api/open", { url: a.href }).catch(() => {});
  }
</script>

<svelte:document onclick={openLinks} />

{#if failure}
  <main><p class="error">{failure}</p></main>
{:else if !ready}
  <main><p class="muted">Loading…</p></main>
{:else}
  <div class="shell">
    <aside class="sidebar">
      <a class="brand" href="#/home"><img src="./icon.png" alt="" /><span>{app.info!.name}</span></a>
      {#if app.info!.games.length > 1}
        <select aria-label="Game" value={app.game} onchange={(e) => switchGame(e.currentTarget.value)}>
          {#each app.info!.games as g}<option value={g.ID}>{g.Name}</option>{/each}
        </select>
      {/if}
      <nav id="nav" aria-label="Main">
        {#each NAV as [v, icon, label]}
          <a href="#/{v}" class:active={lit === v}><Icon name={icon} />{v === "collection" ? gameInfo().Name.split(":")[0] : label}</a>
        {/each}
      </nav>
      <div class="side-actions">
        <button class="button primary" type="button" onclick={() => openAdd()}><Icon name="add" />Add…</button>
        {#if gameInfo().Scan}<a class="button" href="#/scan"><Icon name="scan" />Scan a card</a>{/if}
      </div>
      <p class="side-foot muted">{app.info!.name} {app.info!.version}</p>
    </aside>
    <div class="content">
      <header class="topbar">
        <a class="brand phone-only" href="#/home"><img src="./icon.png" alt="" /><span>{app.info!.name}</span></a>
        <GlobalSearch />
      </header>
      <main>
        {#key `${app.game}`}
          <Page params={route.params} />
        {/key}
      </main>
      <footer id="status" role="status" aria-live="polite" class:error={app.status.error}>
        {#if app.status.progress?.total}<progress value={app.status.progress.done} max={app.status.progress.total}></progress>{/if}
        {app.status.message}
        {#if app.status.undo}
          {" "}<button type="button" class="link" onclick={() => undoChange(app.status.undo!)}>Undo</button>
        {/if}
      </footer>
    </div>
    <nav class="tabbar" aria-label="Main">
      <a href="#/home" class:active={lit === "home"}><Icon name="home" />Home</a>
      <a href="#/collection" class:active={lit === "collection"}><Icon name="cards" />Cards</a>
      <a href="#/add" onclick={(e) => { e.preventDefault(); openAdd(); }}><Icon name="add" />Add</a>
      <a href="#/lists" class:active={lit === "lists"}><Icon name="lists" />Lists</a>
      <a href="#/tools" class:active={lit === "tools" || lit === "stats"}><Icon name="tools" />Tools</a>
      <a href="#/settings" class:active={lit === "settings"}><Icon name="settings" />Settings</a>
    </nav>
  </div>
  {#each dialogs as d (d.id)}
    <d.component {...d.props} done={d.done} />
  {/each}
{/if}

<style>
  /* The frame: a sidebar on desktop, a tab bar on phones. */
  .shell { display: grid; grid-template-columns: var(--sidebar) minmax(0, 1fr); min-height: 100vh; }
  .sidebar {
    position: sticky; top: 0; height: 100vh; display: flex; flex-direction: column; gap: 14px;
    padding: max(16px, env(safe-area-inset-top)) 12px 16px; background: var(--panel); border-right: 1px solid var(--line);
  }
  .brand { display: flex; align-items: center; gap: 9px; color: var(--text); font-weight: 700; text-decoration: none; font-size: 16px; padding: 2px 6px; }
  .brand img { width: 30px; height: 30px; border-radius: 7px; }
  #nav { display: grid; gap: 2px; }
  #nav a { display: flex; align-items: center; gap: 10px; padding: 8px 10px; border-radius: 8px; color: var(--muted); text-decoration: none; font-weight: 500; }
  #nav a:hover { color: var(--text); background: var(--panel-2); }
  #nav a.active { background: var(--blue-soft); color: var(--text); }
  .side-actions { display: grid; gap: 8px; }
  .side-actions :global(.ico) { width: 18px; height: 18px; }
  .side-foot { margin-top: auto; font-size: 12px; padding: 0 6px; }
  .content { display: flex; flex-direction: column; min-width: 0; }
  .topbar {
    display: flex; align-items: center; gap: 12px; padding: 10px 20px; position: sticky; top: 0; z-index: 5;
    background: color-mix(in srgb, var(--bg) 88%, transparent); backdrop-filter: blur(10px); border-bottom: 1px solid var(--line);
  }
  main { flex: 1; padding: 18px 20px 24px; width: 100%; max-width: 1440px; }
  /* The status line: what the last change did, with Undo, or a long job's progress. */
  #status { padding: 8px 20px; color: var(--muted); font-size: 13px; min-height: 2.2em; }
  #status.error { color: var(--red); }
  #status progress { width: 140px; height: 8px; margin-right: 10px; vertical-align: middle; accent-color: var(--blue); }
  .tabbar, .phone-only { display: none; }
  @media (max-width: 800px) {
    .shell { grid-template-columns: 1fr; padding-bottom: calc(var(--tabbar) + env(safe-area-inset-bottom)); }
    .sidebar { display: none; }
    .phone-only { display: flex; }
    .topbar { padding: max(8px, env(safe-area-inset-top)) 14px 8px; }
    .topbar .brand span { display: none; }
    main { padding: 14px 14px 20px; }
    #status { padding: 6px 14px; }
    .tabbar {
      display: grid; grid-template-columns: repeat(6, 1fr); position: fixed; left: 0; right: 0; bottom: 0; z-index: 10;
      height: calc(var(--tabbar) + env(safe-area-inset-bottom)); padding-bottom: env(safe-area-inset-bottom);
      background: color-mix(in srgb, var(--panel) 92%, transparent); backdrop-filter: blur(12px); border-top: 1px solid var(--line);
    }
    .tabbar a { display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 1px; font-size: 11px; color: var(--muted); text-decoration: none; }
    .tabbar a :global(.ico) { width: 24px; height: 24px; }
    .tabbar a.active { color: var(--blue); }
  }
</style>
