<!-- What importing a file (or a pasted list) would do, before anything is saved, with
     Import and Cancel. With `lot`, the cards go into that open lot. -->
<script lang="ts" module>
  export interface Preview {
    token: string; format: string; rows: number; cards: number; copies: number; newCards: number; alreadyOwned: number;
    pricePaid: number; notes: string[]; unmatched: { line: number; text: string }[]; header: string;
  }
</script>

<script lang="ts">
  import { api, gameAPI, loadFilters, status, whileRunning } from "../lib/app.svelte";
  import { plural, usd } from "../lib/format";
  import type { Lot } from "../lib/types";
  import TagEditor from "./TagEditor.svelte";
  import Tile from "./Tile.svelte";

  let { preview, fileName, lot = null, cancel, done }: {
    preview: Preview; fileName: string; lot?: Lot | null; cancel: () => void; done?: (r: any) => void;
  } = $props();

  let tags = $state<string[]>([]);
  let editor = $state<ReturnType<typeof TagEditor>>();
  let busy = $state(false);
  let result = $state<{ summary: string } | null>(null);

  // The rows that matched no card, as a CSV with the file's header, to fix and import.
  const unmatchedUrl = $derived(URL.createObjectURL(new Blob([[preview.header, ...preview.unmatched.map((u) => u.text)].filter(Boolean).join("\n") + "\n"], { type: "text/csv" })));

  async function commit() {
    if (editor && !editor.add()) return;
    busy = true;
    status(`Importing ${plural(preview.copies, "card", "cards")}…`);
    try {
      const body: Record<string, unknown> = { token: preview.token, tags };
      if (lot) body.lot = lot.id;
      const r = await whileRunning(api("POST", gameAPI("/import/commit"), body));
      status(`Imported ${plural(r.cards, "card", "cards")} from ${r.format}${lot ? ` into ${lot.name}` : ""}.`, false, r.undo);
      result = r;
      await loadFilters();
      done?.(r);
    } catch (e) { status(e instanceof Error ? e.message : String(e), true); busy = false; }
  }
</script>

{#if result}
  <h2>Import Complete</h2>
  <pre class="summary">{result.summary}</pre>
{:else}
  <h2>Preview: {fileName}</h2>
  <p class="muted">{preview.format} · {plural(preview.rows, "row", "rows")} read. Nothing has been saved yet.</p>
  <div class="tiles">
    <Tile cap="CARDS" value={preview.copies.toLocaleString()} detail={plural(preview.cards, "printing", "printings")} />
    <Tile cap="NEW" value={preview.newCards.toLocaleString()} detail="cards you don't own yet" />
    <Tile cap="ALREADY OWNED" value={preview.alreadyOwned.toLocaleString()} detail="add to cards you own" />
    <Tile cap="PRICE PAID" value={usd(preview.pricePaid)} detail="for cards with a price" />
  </div>
  {#if preview.unmatched.length}
    <p><strong>{plural(preview.unmatched.length, "row", "rows")} matched no card and won't be imported.</strong> Download them, fix them, and import them again.</p>
    <p><a class="button" href={unmatchedUrl} download={fileName.replace(/\.[^.]*$/, "") + "-unmatched.csv"}>Download the unmatched rows</a></p>
    <details><summary>Show the unmatched rows</summary><pre class="raw">{preview.unmatched.map((u) => `Line ${u.line}: ${u.text}`).join("\n")}</pre></details>
  {/if}
  {#if preview.notes.length}<pre class="summary">{preview.notes.join("\n")}</pre>{/if}
  {#if preview.copies}
    <div class="import-tags"><span>Tag every card in this import (optional)</span><TagEditor bind:this={editor} bind:tags label="Tag every card in this import" /></div>
  {/if}
  <div class="buttons"><div class="right">
    {#if preview.copies}<button type="button" class="primary" disabled={busy} onclick={commit}>Import {plural(preview.copies, "card", "cards")}</button>{/if}
    <button type="button" onclick={cancel}>Cancel</button>
  </div></div>
{/if}

<style>
  .import-tags { display: grid; gap: 6px; margin: 12px 0; max-width: 520px; font-size: 14px; }
  pre.summary { background: var(--panel); border: 1px solid var(--line); border-radius: var(--radius); padding: 12px; white-space: pre-wrap; }
</style>
