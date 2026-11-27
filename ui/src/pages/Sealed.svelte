<!-- Sealed product: boxes, bundles and decks kept unopened, valued by hand. -->
<script lang="ts">
  import { api, app, failed, gameAPI, reload, status } from "../lib/app.svelte";
  import { ask, openDialog } from "../lib/dialogs.svelte";
  import { gain, plural, signedUsd, usd } from "../lib/format";
  import Tile from "../components/Tile.svelte";
  import SealedDialog, { productKind, type SealedItem } from "../components/SealedDialog.svelte";

  interface Sealed { items: SealedItem[]; totals: { items: number; paid: number; value: number; unvalued: number; stale: number } }
  const data = $derived.by(() => { void app.reloads; return api<Sealed>("GET", gameAPI("/sealed")); });

  async function edit(item?: SealedItem) { if (await openDialog(SealedDialog, { item: item || null })) reload(); }
  async function remove(i: SealedItem) {
    if (!(await ask(`Remove ${i.name} from your sealed product?`, "Remove"))) return;
    try { await api("DELETE", gameAPI(`/sealed?id=${encodeURIComponent(i.id)}`)); status(`Removed ${i.name}.`); reload(); } catch (e) { failed(e); }
  }
</script>

<div class="section-head"><h1>Sealed product</h1><button type="button" class="primary" onclick={() => edit()}>Add sealed product</button></div>
<p class="muted">Boxes, bundles and decks you keep unopened. Values are entered by hand (no free source prices sealed product) and aren't part of the cards' market value on Home.</p>
{#await data then d}
  {@const t = d.totals}
  {@const both = d.items.filter((i) => i.gain !== null)}
  {@const g = both.length ? gain(both.reduce((n, i) => n + i.price_paid * i.quantity, 0), both.reduce((n, i) => n + i.value * i.quantity, 0)) : null}
  {@const left = d.items.length - both.length}
  <div class="tiles">
    <Tile cap="ITEMS" value={t.items.toLocaleString()} detail={t.unvalued ? `${t.unvalued} without a value` : ""} />
    <Tile cap="PRICE PAID" value={usd(t.paid)} />
    <Tile cap="VALUE" value={usd(t.value)} detail={t.stale ? `${plural(t.stale, "value", "values")} to enter or check` : "every value is recent"} color="var(--blue)" />
    <Tile cap="GAIN / LOSS" value={g ? g.text : "—"} color={g?.color || ""}
      detail={g ? [g.pct, left ? `${plural(left, "item", "items")} without a price paid or value left out` : ""].filter(Boolean).join(" · ") : "needs prices paid and values"} />
  </div>
  {#if d.items.length}
    <div class="card-box">
      {#each d.items as i (i.id)}
        <div class="list-entry" class:stale={i.valueStale}>
          <div class="text">
            <span class="name">{i.name}</span>
            <span class="meta">{[i.set, productKind(i), `×${i.quantity}`, i.price_paid ? `paid ${usd(i.price_paid)} each` : "no price paid", i.acquired_date, i.note].filter(Boolean).join(" · ")}</span>
            <span class="meta">
              {i.value ? `worth ${usd(i.value)} each, as of ${i.value_date || "—"}${i.valueStale ? " · over 90 days old: check it" : ""}` : "no value yet: look it up and enter it"}
              {#if i.purchase_url} · <a href={i.purchase_url} target="_blank" rel="noopener noreferrer">Look up a price ↗</a>{/if}
            </span>
          </div>
          <div class="entry-controls">
            <span class="change" class:up={(i.gain || 0) > 0} class:down={(i.gain || 0) < 0}>{i.gain === null ? (i.value ? usd(i.value * i.quantity) : "—") : signedUsd(i.gain)}</span>
            <button type="button" onclick={() => edit(i)}>Edit…</button>
            <button type="button" aria-label="Remove {i.name}" title="Remove" onclick={() => remove(i)}>✕</button>
          </div>
        </div>
      {/each}
    </div>
  {:else}<p class="muted">No sealed product yet.</p>{/if}
{:catch e}<p class="error">{e.message}</p>{/await}

<style>
  /* An item whose value is missing or over 90 days old. */
  .list-entry.stale { box-shadow: inset 3px 0 0 var(--amber); padding-left: 8px; }
</style>
