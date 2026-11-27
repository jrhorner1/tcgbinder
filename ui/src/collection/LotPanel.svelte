<!-- A lot's figures and actions, above the collection when it's filtered to the lot. -->
<script lang="ts">
  import { api, failed, gameAPI, loadFilters, loadLots, reload, status } from "../lib/app.svelte";
  import { askDialog, openDialog } from "../lib/dialogs.svelte";
  import { gain, plural, usd, usd4 } from "../lib/format";
  import { finishLot, lotCountText, lotProgress } from "../lib/lots";
  import { go } from "../lib/router.svelte";
  import type { Lot } from "../lib/types";
  import { openAdd } from "../add/add";
  import Tile from "../components/Tile.svelte";
  import LotDialog from "./LotDialog.svelte";

  let { lot }: { lot: Lot } = $props();

  const g = $derived(gain(lot.total_paid, lot.marketValue));
  const path = $derived(gameAPI(`/lots?id=${encodeURIComponent(lot.id)}`));
  const act = (fn: () => Promise<void>) => async () => { try { await fn(); } catch (e) { failed(e); } };

  const reopen = act(async () => { await api("PUT", path, { open: true }); await loadLots(); openAdd({ lot: lot.id }); });
  const reprice = act(async () => {
    const r = await api("PUT", path, { recalculate: true });
    status(`Repriced ${r.lot.name} at today's prices.${r.note ? " " + r.note : ""}`);
    await loadLots();
    reload();
  });
  const remove = act(async () => {
    const what = await askDialog(`Delete ${lot.name}?`, ["Keep its cards (with the prices they have now), or delete them from your collection too?"],
      [{ label: "Delete the cards too", value: "remove", danger: true }, { label: "Keep the cards", value: "keep", primary: true }]);
    if (!what) return;
    const r = await api("DELETE", `${path}&cards=${what}`);
    status(`Deleted the lot ${lot.name}${what === "remove" ? " and its cards" : ""}.`, false, r.undo);
    await loadLots();
    await loadFilters();
    go("collection");
  });
  const edit = async () => { if (await openDialog(LotDialog, { lot })) { await loadLots(); reload(); } };
  const finish = async () => { if (await finishLot(lot)) reload(); };
</script>

<div class="card-box lot-panel">
  <div class="lot-head">
    <div>
      <h2>📦 {lot.name}</h2>
      <p class="muted">
        {[lot.acquired_date, lotCountText(lot), lot.split === "even" ? "split evenly" : "split by market value",
          lot.open ? "being entered" : lot.split_date ? `split on ${lot.split_date}` : "finished"].filter(Boolean).join(" · ")}
      </p>
    </div>
    <div class="buttons">
      {#if lot.open}
        <button type="button" class="primary" onclick={() => openAdd({ lot: lot.id })}>Keep adding</button>
        <button type="button" onclick={finish}>Finish</button>
      {:else}
        <button type="button" onclick={reopen}>Add more</button>
        {#if lot.split === "by_value"}<button type="button" onclick={reprice}>Reprice at today's prices</button>{/if}
      {/if}
      <button type="button" onclick={edit}>Edit</button>
      <button type="button" class="danger" onclick={remove}>Delete</button>
    </div>
  </div>
  <div class="tiles">
    <Tile cap="PAID" value={usd(lot.total_paid)} detail={lot.priceEach !== null ? `${usd4(lot.priceEach)} each` : plural(lot.copies, "card", "cards")} />
    <Tile cap="WORTH NOW" value={usd(lot.marketValue)} detail={plural(lot.copies, "card", "cards")} color="var(--blue)" />
    <Tile cap="ENTERED" value={lot.expected !== null ? `${lot.entered} / ${lot.expected}` : lot.entered.toLocaleString()} detail={lotProgress(lot)} />
    <Tile cap="GAIN / LOSS" value={g.text} detail={g.pct} color={g.color} />
  </div>
</div>

<style>
  .lot-panel { margin-bottom: 14px; }
  .lot-head { display: flex; gap: 12px; align-items: flex-start; justify-content: space-between; flex-wrap: wrap; margin-bottom: 12px; }
  .lot-head h2 { margin: 0 0 4px; }
  .lot-head p { margin: 0; }
  .lot-head .buttons { display: flex; gap: 6px; flex-wrap: wrap; margin: 0; }
  @media (max-width: 800px) { .lot-panel .tiles { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
</style>
