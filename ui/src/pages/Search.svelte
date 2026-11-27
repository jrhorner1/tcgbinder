<!-- Search results across every game: the cards you own that match, and searching the
     catalog to add more. -->
<script lang="ts">
  import { api, app, loadFilters, loadLots, store } from "../lib/app.svelte";
  import { copiesSummary } from "../lib/cards";
  import { plural, sn } from "../lib/format";
  import { go } from "../lib/router.svelte";
  import type { Card } from "../lib/types";
  import { openAdd } from "../add/add";

  let { params }: { params: URLSearchParams } = $props();

  interface GameHits { Game: string; Name: string; Cards: Card[]; Total: number }
  const q = $derived(params.get("q") || "");
  const found = $derived(api<{ games: GameHits[] }>("GET", `/api/search?q=${encodeURIComponent(q)}`));

  async function switchTo(id: string) {
    if (app.game === id) return;
    app.game = id;
    store("game", id);
    await Promise.all([loadFilters(), loadLots()]);
  }
</script>

<h1>Search: “{q}”</h1>
{#await found}
  <p class="muted">Searching…</p>
{:then r}
  {#if !r.games.reduce((n, g) => n + g.Total, 0)}<p class="muted">No cards in your collection match. Search the catalog to add one.</p>{/if}
  {#each r.games as g (g.Game)}
    {#if r.games.length > 1 || g.Total}<h2>{g.Name} — {plural(g.Total, "match", "matches")}</h2>{/if}
    {#if g.Cards.length}
      <div class="list">
        {#each g.Cards as c (c.key)}
          <button type="button" class="card" onclick={async () => { await switchTo(g.Game); go("collection", { key: c.key }); }}>
            <span class="text"><span class="name">{c.name}</span><span class="meta">{sn(c)} · {copiesSummary(c)}</span></span>
          </button>
        {/each}
      </div>
    {/if}
    {#if g.Total > g.Cards.length}
      <p><span class="muted">Showing the first {g.Cards.length}. </span>
        <button type="button" class="link" onclick={async () => { await switchTo(g.Game); go("collection", { q }); }}>Show all {g.Total.toLocaleString()}</button></p>
    {/if}
    <p><button type="button" onclick={async () => { await switchTo(g.Game); openAdd({ query: q }); }}>Search the {g.Name} catalog for “{q}”</button></p>
  {/each}
{:catch e}
  <p class="error">{e.message}</p>
{/await}
