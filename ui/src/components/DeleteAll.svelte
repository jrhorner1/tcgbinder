<!-- Deleting all data: every game's collection, lots, sales, lists, watchlist, sealed
     product and price history. "delete" must be typed, and a backup of the collection
     is kept unless unticked. Answers true once deleted. -->
<script lang="ts">
  import { api, status } from "../lib/app.svelte";
  import Modal from "./Modal.svelte";

  let { done }: { done: (v: boolean | null) => void } = $props();
  let word = $state("");
  let keep = $state(true);
  let busy = $state(false);
  let error = $state("");

  async function go() {
    error = "";
    busy = true;
    try {
      const r = await api("POST", "/api/data/delete-all", { confirm: word, keepBackup: keep });
      status(Object.keys(r.backups).length ? "Deleted all data. The collection's backup is under Tools → Restore a backup." : "Deleted all data.");
      done(true);
    } catch (e) { error = e instanceof Error ? e.message : String(e); busy = false; }
  }
</script>

<Modal title="Delete all data?" small {done}>
  <p>This deletes every card in your collection, your lots, sales, lists, watchlist, sealed product and price history, for every game. Your settings stay.</p>
  <p><strong>It can't be undone</strong> unless you keep a backup of the collection below. Lots, sales, lists, the watchlist and sealed product are not in the backup.</p>
  <label class="check"><input type="checkbox" bind:checked={keep} /> Keep a backup of the collection (restore it from Tools → Restore a backup)</label>
  <p>Type <strong>delete</strong> to confirm:</p>
  <input type="text" placeholder="delete" aria-label="Type delete to confirm" autocomplete="off" bind:value={word} />
  <p class="error">{error}</p>
  {#snippet footer()}
    <button type="button" onclick={() => done(null)}>Cancel</button>
    <button type="button" class="danger" disabled={busy || word.trim().toLowerCase() !== "delete"} onclick={go}>Delete all data</button>
  {/snippet}
</Modal>
