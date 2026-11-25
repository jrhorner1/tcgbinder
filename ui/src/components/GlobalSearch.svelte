<!-- The search at the top of every page: as you type, your cards that match (by name,
     set, number, tag or rules text) and the catalog's (by name, then by rules text);
     Enter shows the full search page. "/" jumps here from anywhere. -->
<script lang="ts">
  import { api, app, gameAPI } from "../lib/app.svelte";
  import { sn } from "../lib/format";
  import { go, route } from "../lib/router.svelte";
  import type { Card, CatalogCard } from "../lib/types";
  import { openAdd } from "../add/add";
  import { copiesSummary } from "../lib/cards";
  import Thumb from "./Thumb.svelte";

  interface Mine { Game: string; Cards: Card[]; Total: number }
  let q = $state("");
  let open = $state(false);
  let mine = $state<Mine>({ Game: "", Cards: [], Total: 0 });
  let catalog = $state<{ cards: CatalogCard[] } | { error: string } | null>(null);
  // Catalog cards whose rules text has the words, one per card.
  let byRules = $state<CatalogCard[]>([]);
  let input: HTMLInputElement;
  let asked = 0;
  let timer: ReturnType<typeof setTimeout>;

  async function run() {
    const text = q.trim();
    if (text.length < 2) { open = false; return; }
    const n = ++asked;
    const catalogSearch = (rules: boolean) => gameAPI(`/search?${new URLSearchParams({ text, ...(rules ? { rules: "1" } : {}) })}`);
    const [m, c, r] = await Promise.all([
      api<{ games: Mine[] }>("GET", `/api/search?q=${encodeURIComponent(text)}`).catch(() => ({ games: [] as Mine[] })),
      text.length >= 3 ? api<{ cards: CatalogCard[] }>("GET", catalogSearch(false)).catch((e) => ({ error: e.message })) : null,
      text.length >= 3 ? api<{ cards: CatalogCard[] }>("GET", catalogSearch(true)).catch(() => ({ cards: [] })) : { cards: [] },
    ]);
    if (n !== asked) return; // a later search is on its way
    mine = m.games.find((g) => g.Game === app.game) || { Game: "", Cards: [], Total: 0 };
    catalog = c;
    // Cards the name search already shows aren't listed again.
    const named = new Set(c && "cards" in c ? c.cards.map((x) => x.name) : []);
    byRules = r.cards.filter((x) => !named.has(x.name));
    open = true;
  }
  // The page the search page was opened from: clearing the box goes back there.
  let searchedFrom = "#/collection";

  function typed() {
    clearTimeout(timer);
    if (!q.trim() && route.view === "search") { open = false; location.hash = searchedFrom; return; }
    timer = setTimeout(run, 300);
  }

  function submit(e: SubmitEvent) {
    e.preventDefault();
    hide();
    if (!q.trim()) return;
    if (route.view !== "search") searchedFrom = location.hash || "#/collection";
    go("search", { q: q.trim() });
  }
  const hide = () => { open = false; };

  // A card of yours found by its rules text rather than its name says so.
  const words = $derived(q.trim().toLowerCase().split(/\s+/).filter(Boolean));
  const inRulesOnly = (c: Card) => !words.every((w) => c.name.toLowerCase().includes(w))
    && words.some((w) => (c.oracle_text || "").toLowerCase().includes(w));

  function keys(e: KeyboardEvent) {
    const t = e.target as HTMLElement;
    if (e.key === "/" && !t.closest("input, select, textarea, dialog")) { e.preventDefault(); input.focus(); }
  }
</script>

<svelte:document onkeydown={keys} onclick={(e) => { if (!(e.target as HTMLElement).closest("#globalsearch")) hide(); }} />

<form id="globalsearch" role="search" onsubmit={submit}>
  <input bind:this={input} type="search" bind:value={q} oninput={typed} placeholder="Search your cards and the catalog" aria-label="Search your cards and the catalog"
    autocomplete="off" onfocus={() => { if (q.trim().length >= 2) open = true; }} onkeydown={(e) => { if (e.key === "Escape") { hide(); input.blur(); } }} />
  {#if open}
    <div class="gresults">
      <h4>In your collection{mine.Total ? ` (${mine.Total})` : ""}</h4>
      {#each mine.Cards.slice(0, 5) as c (c.key)}
        <button type="button" class="gresult" onclick={() => { hide(); go("collection", { key: c.key }); }}>
          <Thumb card={c} size={30} />
          <span class="text"><span class="name">{c.name}</span><span class="meta">{sn(c)}{inRulesOnly(c) ? " · in its rules text" : ""}</span></span>
          <span class="muted small">{copiesSummary(c)}</span>
        </button>
      {:else}
        <p class="none">None of your cards match.</p>
      {/each}
      {#if mine.Total > 5}
        <button type="button" class="link gmore" onclick={() => { hide(); if (route.view !== "search") searchedFrom = location.hash || "#/collection"; go("search", { q: q.trim() }); }}>Show all {mine.Total}</button>
      {/if}
      <h4>In the catalog</h4>
      {#if !catalog}
        <p class="none">Type a little more to search the catalog.</p>
      {:else if "error" in catalog}
        <p class="none">{catalog.error}</p>
      {:else}
        {#each catalog.cards.slice(0, 6) as c (c.id)}
          <div class="gresult">
            <button type="button" class="open" onclick={() => { hide(); go("collection", { key: c.id }); }}>
              <Thumb card={c} size={30} />
              <span class="text"><span class="name">{c.name}</span><span class="meta">{sn(c)}</span></span>
            </button>
            <button type="button" class="small" onclick={() => { hide(); openAdd({ card: c }); }}>Add…</button>
          </div>
        {:else}
          <p class="none">No card names match.</p>
        {/each}
        {#if byRules.length}
          <h4>Rules text mentions it</h4>
          {#each byRules.slice(0, 5) as c (c.id)}
            <div class="gresult">
              <button type="button" class="open" onclick={() => { hide(); go("collection", { key: c.id }); }}>
                <Thumb card={c} size={30} />
                <span class="text"><span class="name">{c.name}</span><span class="meta rules">{c.oracle_text}</span></span>
              </button>
              <button type="button" class="small" onclick={() => { hide(); openAdd({ card: c }); }}>Add…</button>
            </div>
          {/each}
        {/if}
      {/if}
    </div>
  {/if}
</form>

<style>
  #globalsearch { position: relative; flex: 1; max-width: 640px; }
  #globalsearch input { width: 100%; padding: 8px 12px; border-radius: 9px; }
  /* The results as you type, under the search box. */
  .gresults { position: absolute; left: 0; right: 0; top: calc(100% + 6px); z-index: 30; background: var(--panel); border: 1px solid var(--line); border-radius: 12px;
    box-shadow: 0 12px 30px rgb(0 0 0 / 35%); max-height: 70vh; overflow: auto; padding: 6px 0; }
  .gresults h4 { margin: 8px 14px 4px; font-size: 12px; text-transform: uppercase; letter-spacing: .04em; color: var(--muted); }
  .gresults .none { margin: 2px 14px 8px; color: var(--muted); font-size: 14px; }
  .gresults .gmore { margin: 2px 14px 6px; }
  .gresult { display: flex; gap: 10px; align-items: center; width: 100%; border: 0; border-radius: 0; background: none; padding: 6px 14px; text-align: left; justify-content: flex-start; }
  .gresult:hover, .gresult:focus-visible, .gresult:focus-within { background: var(--panel-2); }
  /* A catalog result: the card (opening its page) and Add beside it. */
  div.gresult { padding-right: 14px; }
  .gresult .open { display: flex; gap: 10px; align-items: center; flex: 1; min-width: 0; border: 0; border-radius: 0; background: none; padding: 0; text-align: left; justify-content: flex-start; white-space: normal; }
  .gresult .text { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .gresult .meta { color: var(--muted); font-size: 13px; }
  /* A card's rules text, on one line. */
  .gresult .meta.rules { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .small { font-size: 12px; white-space: nowrap; }
</style>
