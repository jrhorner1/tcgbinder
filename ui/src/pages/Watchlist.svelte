<!-- The watchlist: watched prices, how far each has moved, and its alert. -->
<script lang="ts">
  import { api, app, gameAPI, reload } from "../lib/app.svelte";
  import { openDialog } from "../lib/dialogs.svelte";
  import { fname, sn, usd } from "../lib/format";
  import { ALERT_NOTE, ruleText, watchAction, watchChange, type Watch } from "../lib/watchlist";
  import WatchDialog from "../card/WatchDialog.svelte";
  import Thumb from "../components/Thumb.svelte";

  const list = $derived.by(() => { void app.reloads; return api<Watch[]>("GET", gameAPI("/watchlist")); });
  const change = async (w: Watch) => { if (await openDialog(WatchDialog, { card: w.card, watch: w })) reload(); };
</script>

<h1>Watchlist</h1>
<p class="muted">{ALERT_NOTE}</p>
{#await list}
  <p class="muted">Loading…</p>
{:then watches}
  {#if watches.length}
    <div class="card-box">
      {#each watches as w (w.key)}
        <div class="list-entry" class:alerted={!!w.triggered}>
          <Thumb card={w.card} />
          <div class="text">
            <span class="name">{w.card.name}</span>
            <span class="meta">{[sn(w.card), fname(w.finish), `since ${w.baseline_date}: ${w.baseline ? usd(w.baseline) : "no price"} → ${w.price !== null ? usd(w.price) : "no price"}`].join(" · ")}</span>
            <span class="meta">Alert: {ruleText(w.rule)}{w.triggered ? ` · triggered ${w.triggered.date} at ${usd(w.triggered.price)}` : ""}</span>
          </div>
          <div class="entry-controls">
            <span class="change" class:up={(w.change || 0) > 0} class:down={(w.change || 0) < 0}>{watchChange(w) || "—"}</span>
            <button type="button" onclick={() => change(w)}>Change…</button>
            {#if w.triggered}<button type="button" onclick={() => watchAction(w, "dismiss")}>Dismiss</button>{/if}
            <button type="button" title="Measure from today's price" onclick={() => watchAction(w, "reset")}>Reset</button>
            <button type="button" aria-label="Stop watching {w.card.name}" title="Stop watching" onclick={() => watchAction(w, "remove")}>✕</button>
          </div>
        </div>
      {/each}
    </div>
  {:else}
    <p class="muted">Nothing watched yet. Choose Watch price… on a card's page.</p>
  {/if}
{:catch e}
  <p class="error">{e.message}</p>
{/await}

<style>
  .list-entry.alerted { box-shadow: inset 3px 0 0 var(--amber); padding-left: 8px; }
</style>
