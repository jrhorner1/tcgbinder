<!-- A set's completion and value, above the collection when it's filtered to the set,
     with the cards you're missing a click away. -->
<script lang="ts">
  import { api, gameAPI } from "../lib/app.svelte";
  import { gain, plural, usd } from "../lib/format";
  import { createMissingList } from "../lib/missing";
  import Tile from "../components/Tile.svelte";

  let { code }: { code: string } = $props();

  interface SetInfo { code: string; name: string; released: string; size: number; collected: number; completion: number; cards: number; uniqueCards: number; marketValue: number; pricePaid: number }
  const info = $derived(api<SetInfo>("GET", gameAPI(`/set?code=${encodeURIComponent(code)}`)));
</script>

{#await info then d}
  {@const g = gain(d.pricePaid, d.marketValue)}
  {@const missing = d.size > 0 ? Math.max(d.size - d.collected, 0) : null}
  <div class="detail set-page">
    <h1>{d.name || d.code}</h1>
    <p class="muted">{[d.code, d.released ? `released ${d.released}` : ""].filter(Boolean).join(" · ")}</p>
    <div class="tiles">
      <Tile cap="COMPLETION" value={d.size > 0 ? `${Math.round(d.completion * 100)}%` : "—"} detail={d.size > 0 ? `${d.collected} of ${d.size} cards` : "set size not loaded yet"} color="var(--blue)" />
      <Tile cap="CARDS" value={d.cards.toLocaleString()} detail={plural(d.uniqueCards, "unique card", "unique cards")} />
      <Tile cap="MARKET VALUE" value={usd(d.marketValue)} detail="price paid {usd(d.pricePaid)}" color="var(--blue)" />
      <Tile cap="GAIN / LOSS" value={g.text} detail={g.pct} color={g.color} />
    </div>
    <div class="missing-actions">
      <a class="button" href="#/missing?{new URLSearchParams({ set: code })}">{missing !== null ? `View the ${missing} missing cards` : "View the missing cards"}</a>
      <button type="button" onclick={() => createMissingList(code, d.name)}>Create a list of missing cards…</button>
    </div>
  </div>
{/await}

<style>
  .set-page { margin-bottom: 14px; }
  .missing-actions { display: flex; gap: 8px; flex-wrap: wrap; align-items: center; margin: 12px 0 0; }
</style>
