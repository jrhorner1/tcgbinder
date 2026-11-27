<!-- A set's missing cards: the cards of ?set= you don't own yet, by number or most
     expensive first. Each can go on a list or be added. -->
<script lang="ts">
  import { api, app, gameAPI } from "../lib/app.svelte";
  import { openDialog } from "../lib/dialogs.svelte";
  import { anyPrice, compareNumber, plural, title, usd } from "../lib/format";
  import { createMissingList, type Missing } from "../lib/missing";
  import type { CatalogCard } from "../lib/types";
  import { openAdd } from "../add/add";
  import AddToList from "../card/AddToList.svelte";

  let { params }: { params: URLSearchParams } = $props();

  const code = $derived(params.get("set") || "");
  const data = $derived.by(() => { void app.reloads; return Promise.all([api<Missing>("GET", gameAPI(`/missing?set=${encodeURIComponent(code)}`)),
    api<{ name: string }>("GET", gameAPI(`/set?code=${encodeURIComponent(code)}`))]); });
  let order = $state<"number" | "price">("number");

  const byNumber = (a: CatalogCard, b: CatalogCard) => compareNumber(a.collector_number, b.collector_number);
  const byPrice = (a: CatalogCard, b: CatalogCard) => (anyPrice(b) || 0) - (anyPrice(a) || 0) || byNumber(a, b);
</script>

{#await data}
  <p class="muted">Loading the set's cards…</p>
{:then [m, d]}
  {@const setName = d.name || code.toUpperCase()}
  <p><a href="#/collection?{new URLSearchParams({ set: code })}">← {setName}</a></p>
  <h1>{setName}: missing cards</h1>
  {#if m.missing.length}
    <p class="muted">{plural(m.missing.length, "card", "cards")} you don't own yet{m.printedSize ? ` (of the ${m.printedSize} printed)` : ""}: {usd(m.totalCost)} for one of each{m.unpriced ? `, ${m.unpriced} without a price` : ""}. Add one here, or add it to a list.</p>
    <div class="buttons missing-actions">
      <select aria-label="Sort the missing cards" bind:value={order}><option value="number">By number</option><option value="price">Most expensive first</option></select>
      <button type="button" class="primary" onclick={() => createMissingList(code, setName, m)}>Create a list of missing cards…</button>
    </div>
    <div class="list missing">
      {#each [...m.missing].sort(order === "price" ? byPrice : byNumber) as c (c.id)}
        <div class="card">
          <span class="num">{c.collector_number}</span>
          <span class="text"><span class="name">{c.name}</span><span class="meta">{[title(c.rarity), anyPrice(c) ? usd(anyPrice(c)) : ""].filter(Boolean).join(" · ")}</span></span>
          <div class="row-actions">
            <button type="button" onclick={() => openDialog(AddToList, { card: c, quantity: 1 })}>To list…</button>
            <button type="button" onclick={() => openAdd({ query: c.name, set: c.set })}>Add…</button>
          </div>
        </div>
      {/each}
    </div>
  {:else}
    <p class="muted">You have every card in this set.</p>
  {/if}
{:catch e}<p class="error">{e.message}</p>{/await}

<style>
  .missing-actions { display: flex; gap: 8px; flex-wrap: wrap; align-items: center; margin: 12px 0; }
  .missing .card { flex-wrap: wrap; cursor: default; }
  .missing .row-actions { margin-left: auto; }
</style>
