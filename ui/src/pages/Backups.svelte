<!-- Backups: the collection and its lots as they were before each import, bulk change,
     restore, deleted lot and deleting all data; restoring one backs up the current
     collection first. Older backups have no lots: restoring one leaves the lots alone. -->
<script lang="ts">
  import { api, app, failed, gameAPI, loadFilters, loadLots, reload, status } from "../lib/app.svelte";
  import { confirmDialog } from "../lib/dialogs.svelte";
  import { plural } from "../lib/format";
  import ToolRow from "../components/ToolRow.svelte";

  interface Backup { name: string; date: string; reason: string; cards: number; lots: number | null }
  const WHY: Record<string, string> = {
    import: "Before an import", "bulk-edit": "Before a bulk edit", "bulk-delete": "Before a bulk delete", restore: "Before a restore", merge: "Before a merge", move: "Before a move",
    "tag-rename": "Before renaming a tag", "tag-delete": "Before deleting a tag", "delete-all": "Before deleting all data", "lot-delete": "Before deleting a lot",
  };
  const list = $derived.by(() => { void app.reloads; return api<Backup[]>("GET", gameAPI("/backups")); });
  let busy = $state("");

  async function restore(b: Backup) {
    if (!(await confirmDialog(`Replace your collection with the backup from ${b.date} (${plural(b.cards, "unique card", "unique cards")})? Your current collection is backed up first, so you can come back to it.`, { action: "Restore" }))) return;
    busy = b.name;
    try {
      const r = await api("POST", gameAPI("/backups/restore"), { name: b.name });
      status(`Restored the backup from ${b.date}: ${plural(r.cards, "unique card", "unique cards")}${b.lots !== null ? ` and ${plural(b.lots, "lot", "lots")}` : ""}.`, false, r.undo);
      await loadFilters();
      await loadLots();
      reload();
    } catch (e) { failed(e); }
    busy = "";
  }
</script>

<h1>Restore a Backup</h1>
<p>A copy of the collection and its lots is saved before every import, bulk delete, restore, deleted lot and other big change. The last 10 are kept. Backups marked "no lots" were taken before lots were kept in backups: restoring one leaves your lots as they are.</p>
{#await list then backups}
  {#if backups.length}
    <div class="card-box">
      {#each backups as b (b.name)}
        <ToolRow about="{b.date} · {WHY[b.reason] || b.reason} · {plural(b.cards, 'unique card', 'unique cards')} · {b.lots === null ? 'no lots' : plural(b.lots, 'lot', 'lots')}">
          <button type="button" disabled={busy === b.name} onclick={() => restore(b)}>Restore</button>
        </ToolRow>
      {/each}
    </div>
  {:else}
    <p class="muted">No backups yet. One is made before your first import or bulk change.</p>
  {/if}
{:catch e}<p class="error">{e.message}</p>{/await}
