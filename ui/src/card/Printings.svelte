<!-- Every printing of a card, in a language; picking one moves every copy there,
     keeping each group's finish, condition, quantity, price paid, date and tags. -->
<script lang="ts">
  import { api, failed, filters, gameAPI, loadFilters, status } from "../lib/app.svelte";
  import { fname, languageName, sn, title } from "../lib/format";
  import type { Card, CatalogCard } from "../lib/types";

  let { card, onmoved }: { card: Card; onmoved: (key: string) => void } = $props();

  let language = $state(card.lang || "en");
  const found = $derived(api<{ printings: CatalogCard[]; current: string }>("GET",
    gameAPI(`/printings?key=${encodeURIComponent(card.key)}&lang=${encodeURIComponent(language)}`)));

  async function move(p: CatalogCard, current: string) {
    if (p.id === current) return;
    try {
      const r = await api("PUT", gameAPI(`/card/printing?key=${encodeURIComponent(card.key)}`), { id: p.id, lang: language });
      status(`Moved ${card.name} to ${sn(p)}${language !== (card.lang || "en") ? ` (${languageName(language)})` : ""}`
        + `${r.finishChanged ? ` (now ${r.finishes.map(fname).join(", ")}: the new printing doesn't come in every finish you had)` : ""}${r.merged ? ", merged with the copies you already had" : ""}`);
      await loadFilters();
      onmoved(r.key);
    } catch (e) { failed(e); }
  }
</script>

<h2>Change printing</h2>
<p class="note">Pick the printing you own; every group of copies moves with its condition, price paid, date and tags. A card's language belongs to its printing, so choose a language to see its printings.</p>
{#if filters().languages.length}
  <label class="inline">Language
    <select bind:value={language}>{#each filters().languages as l}<option value={l.code}>{l.name}</option>{/each}</select>
  </label>
{/if}
{#await found}
  <p class="muted">Finding every printing…</p>
{:then r}
  {#if !r.printings.length}<p class="muted">No printings in {languageName(language)}.</p>{/if}
  <div class="list results">
    {#each r.printings as p (p.id)}
      <button type="button" class="card" class:sel={p.id === r.current} onclick={() => move(p, r.current)}>
        <div class="text">
          <strong>{p.set_name}{p.id === r.current ? "  (current)" : ""}</strong>
          <span class="meta">{[sn(p), title(p.rarity), (p.released_at || "").slice(0, 4), p.finishes.map(fname).join("/")].filter(Boolean).join(" · ")}</span>
        </div>
      </button>
    {/each}
  </div>
{:catch e}
  <p class="error">{e.message}</p>
{/await}

<style>
  label.inline { display: inline-flex; gap: 8px; align-items: center; color: var(--muted); margin-bottom: 8px; }
  .results { max-height: 65vh; }
</style>
