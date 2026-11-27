<!-- Copies still marked as bought in a lot that no longer exists (deleted, or brought
     back by restoring an older backup): recreate the lot from them, or take them out
     of it, keeping their prices. -->
<script lang="ts">
  import { api, app, failed, gameAPI, loadFilters, loadLots, reload, status } from "../lib/app.svelte";
  import { plural, usd } from "../lib/format";

  interface LostLot { id: string; cards: number; copies: number; paid: number; first_date: string; last_date: string; names: string[] }
  const lost = $derived.by(() => { void app.reloads; void app.game; return api<{ lots: LostLot[] }>("GET", gameAPI("/lots/lost")).then((r) => r.lots, () => []); });
  let busy = $state("");

  async function settle(l: LostLot, action: "recover" | "release") {
    busy = l.id;
    try {
      const r = await api("POST", gameAPI("/lots/lost"), { id: l.id, action });
      if (action === "recover") status(`Recreated the lot as ${r.lot.name}: rename it and check its total with Edit on its page.`);
      else status(`Took ${plural(l.copies, "copy", "copies")} out of the missing lot; they keep their prices.`, false, r.undo);
      await loadLots();
      await loadFilters();
      reload();
    } catch (e) { failed(e); }
    busy = "";
  }

  const dates = (l: LostLot) => !l.first_date ? "" : l.first_date === l.last_date ? `, acquired ${l.first_date}` : `, acquired ${l.first_date} to ${l.last_date}`;
</script>

{#await lost then lots}
  {#each lots as l (l.id)}
    <div class="card-box notice">
      <h2>Copies in a lot that no longer exists</h2>
      <p>
        {plural(l.copies, "copy", "copies")} of {plural(l.cards, "card", "cards")} ({l.names.join(", ")}{l.cards > l.names.length ? "…" : ""}{dates(l)})
        are marked as bought in a lot that isn't in your lots any more, usually because it was deleted or an older backup was restored. Together they were costed at {usd(l.paid)}.
      </p>
      <p class="muted">Recreating the lot makes it again from these copies, finished, as "Recovered lot": rename it and set its total with Edit. Taking them out leaves them in no lot, at the prices they have now.</p>
      <div class="buttons"><div class="right">
        <button type="button" disabled={busy === l.id} onclick={() => settle(l, "release")}>Take them out of the lot</button>
        <button type="button" class="primary" disabled={busy === l.id} onclick={() => settle(l, "recover")}>Recreate the lot</button>
      </div></div>
    </div>
  {/each}
{/await}
