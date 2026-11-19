<!-- Watching a card's price: start (card, and maybe its finish), or change a watch's
     alert (`watch`). -->
<script lang="ts" module>
  export interface Rule { kind: "percent" | "above" | "below"; value: number }
  export interface Watch { key: string; finish: string; rule: Rule }
</script>

<script lang="ts">
  import { api, gameAPI, status } from "../lib/app.svelte";
  import { fname, sn, usd } from "../lib/format";
  import type { CatalogCard } from "../lib/types";
  import Modal from "../components/Modal.svelte";

  let { card, finish = "", watch = null, done }: { card: CatalogCard; finish?: string; watch?: Watch | null; done: (v: boolean | null) => void } = $props();

  const priced = (card.finishes.length ? card.finishes : Object.keys(card.market || {}));
  let fin = $state(watch ? watch.finish : finish || priced[0] || "nonfoil");
  let kind = $state<Rule["kind"]>(watch ? watch.rule.kind : "percent");
  let value = $state(watch ? watch.rule.value : 20);
  let error = $state("");

  async function save() {
    error = "";
    if (!(value > 0)) { error = "Type a number above 0."; return; }
    const rule = { kind, value };
    try {
      if (watch) await api("PUT", gameAPI(`/watchlist?key=${encodeURIComponent(watch.key)}`), { rule });
      else await api("POST", gameAPI("/watchlist"), { card, finish: fin, rule });
      status(watch ? `Changed the alert for ${card.name}.` : `Watching ${card.name} (${fname(fin)}). See Tools → Watchlist.`);
      done(true);
    } catch (e) { error = e instanceof Error ? e.message : String(e); }
  }
</script>

<Modal title={watch ? "Change the alert" : "Watch the price"} subtitle="{card.name} · {sn(card)}" small {done}>
  <div class="form">
    <label for="w-fin">Finish</label>
    <select id="w-fin" bind:value={fin} disabled={!!watch}>
      {#each watch ? [watch.finish] : priced as f}<option value={f}>{fname(f)} · {card.market?.[f] ? usd(card.market[f]) : "no price"}</option>{/each}
    </select>
    <label for="w-kind">Alert when</label>
    <select id="w-kind" bind:value={kind}>
      <option value="percent">It moves by (%)</option>
      <option value="above">It reaches or passes ($)</option>
      <option value="below">It falls to or under ($)</option>
    </select>
    <label for="w-value">Amount</label>
    <input id="w-value" type="number" min="0" step="0.01" bind:value={value} />
  </div>
  <p class="note">Prices are checked when the app refreshes them (each day, or as Settings says), so alerts appear while the app is open or the next time you open it. There is no email or text message.</p>
  <p class="error">{error}</p>
  {#snippet footer()}
    <button type="button" onclick={() => done(null)}>Cancel</button>
    <button type="button" class="primary" onclick={save}>{watch ? "Save" : "Watch"}</button>
  {/snippet}
</Modal>
