<!-- Putting cards you already own into a lot from a pasted list: what each line found in
     your collection, before anything moves. Only copies in no lot are taken; copies in
     another lot stay there. Lines not owned can be added as new cards. -->
<script lang="ts" module>
  export interface OwnedFound { key: string; name: string; set: string; number: string; finish: string; quantity: number }
  export interface OwnedLine { line: number; text: string; name: string; wanted: number; found: OwnedFound[]; inLots: number; missing: string }
  export interface OwnedMatch { lines: OwnedLine[]; found: number; missing: string }
</script>

<script lang="ts">
  import { api, gameAPI, loadFilters, status, whileRunning } from "../lib/app.svelte";
  import { fname, plural } from "../lib/format";
  import type { Lot } from "../lib/types";

  let { match, list, lot, cancel, done, addMissing }: {
    match: OwnedMatch; list: string; lot: Lot; cancel: () => void;
    /** After the copies are moved: the lot as it is now, and how many moved. */
    done: (lot: Lot, moved: number) => void;
    /** Adds what isn't owned as new cards: a list of them. */
    addMissing: (list: string) => void;
  } = $props();

  let busy = $state(false);
  const wanted = $derived(match.lines.reduce((n, l) => n + l.wanted, 0));
  const inLots = $derived(match.lines.reduce((n, l) => n + Math.min(l.inLots, l.wanted - count(l)), 0));
  const short = $derived(match.lines.filter((l) => l.missing));
  const count = (l: OwnedLine) => l.found.reduce((n, f) => n + f.quantity, 0);
  const printing = (f: OwnedFound) => `${f.set.toUpperCase()} ${f.number}${f.finish !== "nonfoil" ? ` ${fname(f.finish)}` : ""}`;

  async function apply() {
    busy = true;
    try {
      const r = await whileRunning(api("POST", gameAPI(`/lots/list?id=${encodeURIComponent(lot.id)}`), { list, apply: true }));
      status(`Put ${plural(r.moved, "card", "cards")} you own into ${lot.name}.`, false, r.undo);
      await loadFilters();
      done(r.lot, r.moved);
    } catch (e) { status(e instanceof Error ? e.message : String(e), true); busy = false; }
  }
</script>

<div class="owned">
  <p>
    <strong>{match.found} of {plural(wanted, "card", "cards")} found in your collection</strong>, in no lot.
    {#if inLots}{plural(inLots, "more is", "more are")} in other lots and will stay there.{/if}
    Nothing has moved yet.
  </p>
  <ul class="lines">
    {#each match.lines as l (l.line)}
      {@const n = count(l)}
      <li class:short={!!l.missing}>
        <span class="mark" aria-hidden="true">{l.missing ? (n ? "◐" : "✕") : "✓"}</span>
        <span class="text">{l.text}</span>
        <span class="muted small">
          {n ? l.found.map((f) => `${f.quantity}× ${printing(f)}`).join(", ") : "not found"}{l.missing && n ? ` · ${l.wanted - n} short` : ""}{l.inLots && l.missing ? ` · ${l.inLots} in other lots` : ""}
        </span>
      </li>
    {/each}
  </ul>
  {#if short.length}
    <p class="muted small">A line without a set takes the printings released by the lot's date first. Add a set code to a line, as in "1 Shock (M19)", to choose the printing.</p>
  {/if}
  <div class="buttons"><div class="right">
    {#if match.missing}<button type="button" onclick={() => addMissing(match.missing)}>Add the cards not found as new…</button>{/if}
    {#if match.found}<button type="button" class="primary" disabled={busy} onclick={apply}>Put {plural(match.found, "card", "cards")} in {lot.name}</button>{/if}
    <button type="button" onclick={cancel}>Cancel</button>
  </div></div>
</div>

<style>
  .lines { list-style: none; margin: 8px 0; padding: 0; max-height: 300px; overflow: auto; border: 1px solid var(--line); border-radius: var(--radius); }
  .lines li { display: grid; grid-template-columns: 1.4em minmax(0, 1fr) auto; gap: 8px; padding: 4px 8px; border-bottom: 1px solid var(--line); align-items: baseline; }
  .lines li:last-child { border-bottom: 0; }
  .lines li.short .mark { color: var(--amber); }
  .mark { text-align: center; }
  .small { font-size: 13px; }
</style>
