<!-- Selling copies from one group: how many, for how much each, and the fees. The sale
     is recorded (see Earnings in the API) and the copies leave the collection. -->
<script lang="ts">
  import { api, gameAPI, status } from "../lib/app.svelte";
  import { fname, plural, signedUsd, sn, today } from "../lib/format";
  import type { Card } from "../lib/types";
  import Modal from "../components/Modal.svelte";
  import type { Row } from "./edits.svelte";

  let { card, row, done }: { card: Card; row: Row; done: (sold: boolean | null) => void } = $props();

  let quantity = $state(1);
  let each = $state(row.market ? row.market.toFixed(2) : "");
  let fees = $state("");
  let error = $state("");

  async function sell() {
    error = "";
    if (!(quantity >= 1 && quantity <= row.quantity)) { error = `Sell 1 to ${row.quantity}.`; return; }
    if (each === "") { error = "Type what each copy sold for."; return; }
    try {
      const s = await api("POST", gameAPI(`/card/sell?key=${encodeURIComponent(card.key)}`), {
        finish: row.finish, condition: row.condition, price_paid: row.price_paid || 0, acquired_date: row.acquired_date || "", tags: row.tags,
        quantity, sale_each: parseFloat(each), date: today(), fees: parseFloat(fees) || 0,
      });
      status(`Sold ${plural(quantity, "copy", "copies")} of ${card.name}${s.gain !== null ? ` (${signedUsd(s.gain)})` : ""}.`);
      done(true);
    } catch (e) { error = e instanceof Error ? e.message : String(e); }
  }
</script>

<Modal title="Sell" subtitle="{card.name} · {sn(card)} · {fname(row.finish)}, {row.condition}" small {done}>
  <div class="form">
    <label for="sell-n">Copies sold</label>
    <input id="sell-n" type="number" min="1" max={row.quantity} step="1" bind:value={quantity} />
    <label for="sell-each">Sold for, each</label>
    <input id="sell-each" type="number" min="0" step="0.01" placeholder="0.00" bind:value={each} />
    <label for="sell-fees">Fees in all</label>
    <input id="sell-fees" type="number" min="0" step="0.01" placeholder="0.00" bind:value={fees} />
  </div>
  <p class="note">Of {plural(row.quantity, "copy", "copies")} in this group.</p>
  <p class="error">{error}</p>
  {#snippet footer()}
    <button type="button" onclick={() => done(null)}>Cancel</button>
    <button type="button" class="primary" onclick={sell}>Sell</button>
  {/snippet}
</Modal>
