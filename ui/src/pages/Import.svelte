<!-- Import: a CSV export from another app, or a collection .json from this app. The
     file is previewed first (matched, nothing saved), then imported on request. -->
<script lang="ts">
  import { api, failed, gameAPI, status, whileRunning } from "../lib/app.svelte";
  import ImportPreview, { type Preview } from "../components/ImportPreview.svelte";

  let file: HTMLInputElement;
  let chosen = $state<File | null>(null);
  let busy = $state(false);
  let preview = $state<Preview | null>(null);

  async function read() {
    if (!chosen) { status("Choose a file first."); return; }
    busy = true;
    preview = null;
    status(`Reading ${chosen.name}… matching cards can take a minute for large files.`);
    try {
      preview = await whileRunning(api<Preview>("POST", gameAPI("/import?preview=1"), chosen));
      status("");
    } catch (e) { failed(e); }
    busy = false;
  }
</script>

<h1>Import</h1>
<p>Import a CSV export from another collection app (EchoMTG, ManaBox, Moxfield, Deckbox, TCGplayer, Dragon Shield, Archidekt and others), or a collection .json file exported from this app. Cards you already own add to their quantities.</p>
<p class="muted">You'll see what the file holds before anything is saved.</p>
<div class="card-box">
  <div class="search">
    <input bind:this={file} type="file" accept=".csv,.json,text/csv,application/json" onchange={() => { chosen = file.files?.[0] || null; preview = null; }} />
    <button type="button" class="primary" disabled={busy} onclick={read}>Preview</button>
  </div>
</div>
{#if preview && chosen}
  <ImportPreview {preview} fileName={chosen.name} cancel={() => { preview = null; file.value = ""; chosen = null; }} />
{/if}
