<!-- One list's cards as a table, like the collection's: a row per entry, edited in place
     (how many wanted, the finish, whether any printing counts), sorted by its
     headings, ticked to remove several. Only the rows in view are drawn. -->
<script lang="ts" module>
  import type { CatalogCard } from "../lib/types";
  /** A list entry as GET /lists?id= gives it; `index` is its place in the list. */
  export interface Entry { index: number; card: CatalogCard; quantity: number; any_printing: boolean; finish: string; owned: number; price: number | null }
  /** A change to one entry, by its index. */
  export interface EntryChange { index: number; quantity?: number; any_printing?: boolean; finish?: string; remove?: boolean }
</script>

<script lang="ts">
  import type { SvelteSet } from "svelte/reactivity";
  import { fname, plural, usd } from "../lib/format";
  import Thumb from "../components/Thumb.svelte";
  import ChoiceCell from "../collection/ChoiceCell.svelte";
  import TextCell from "../collection/TextCell.svelte";
  import "../collection/table.css";

  let { entries, sort, dir, selection, onsort, onedit, onopen, onadd }: {
    entries: Entry[];
    sort: string;
    dir: string;
    /** The ticked entries' indexes. */
    selection: SvelteSet<number>;
    onsort: (by: string) => void;
    onedit: (changes: EntryChange[], label: string) => void;
    onopen: (e: Entry) => void;
    onadd: (e: Entry) => void;
  } = $props();

  // ---- Drawing only the rows in view ----
  const ROW_H = 40, OVERSCAN = 600;
  let scroller: HTMLDivElement;
  let scrollTop = $state(0);
  let viewH = $state(800);
  const first = $derived(Math.max(0, Math.floor((scrollTop - OVERSCAN) / ROW_H)));
  const last = $derived(Math.min(entries.length, Math.ceil((scrollTop + viewH + OVERSCAN) / ROW_H)));
  const visible = $derived(entries.slice(first, last));

  const have = (e: Entry) => Math.min(e.owned, e.quantity);
  const totals = $derived(entries.reduce((t, e) => ({ wanted: t.wanted + e.quantity, owned: t.owned + have(e), value: t.value + (e.price || 0) * e.quantity }),
    { wanted: 0, owned: 0, value: 0 }));
  const finishes = (e: Entry): [string, string][] => [["", "Any"], ...(e.card.finishes.length ? e.card.finishes : ["nonfoil"]).map((f) => [f, fname(f)] as [string, string])];

  const allTicked = $derived(entries.length > 0 && entries.every((e) => selection.has(e.index)));
  function tickAll() {
    if (allTicked) for (const e of entries) selection.delete(e.index);
    else for (const e of entries) selection.add(e.index);
  }

  export function toTop() { scroller.scrollTo({ top: 0, behavior: "smooth" }); }
</script>

{#snippet head(by: string, label: string, cls = "")}
  <th class={cls} class:sorted={sort === by} aria-sort={sort === by ? (dir === "asc" ? "ascending" : "descending") : undefined}>
    <button type="button" class="sorter" onclick={() => onsort(by)}>{label}{sort === by ? (dir === "asc" ? " ▲" : " ▼") : ""}</button>
  </th>
{/snippet}

<div class="table-scroll" bind:this={scroller} bind:clientHeight={viewH} onscroll={() => { scrollTop = scroller.scrollTop; }}>
  <table class="grid">
    <thead>
      <tr>
        <th class="c-tick"><input type="checkbox" aria-label="Select every card listed" checked={allTicked} onchange={tickAll} /></th>
        {@render head("name", "Name", "c-name stick")}
        {@render head("set", "Set", "c-set")}
        {@render head("qty", "Wanted", "c-qty r")}
        {@render head("owned", "Owned", "c-owned r")}
        <th class="c-finish">Finish</th>
        <th class="c-any">Any printing</th>
        {@render head("price", "Price each", "c-price r")}
        {@render head("value", "Value", "c-value r")}
        <th class="c-acts"><span class="sr">Actions</span></th>
      </tr>
    </thead>
    <tbody>
      {#if first > 0}<tr class="spacer" aria-hidden="true"><td colspan="10" style:height="{first * ROW_H}px"></td></tr>{/if}
      {#each visible as e (e.index)}
        {@const done = have(e) >= e.quantity}
        <tr class="card" class:selected={selection.has(e.index)}>
          <td class="c-tick"><input type="checkbox" checked={selection.has(e.index)} aria-label="Select {e.card.name}"
            onchange={() => (selection.has(e.index) ? selection.delete(e.index) : selection.add(e.index))} /></td>
          <td class="c-name stick">
            <div class="namecell">
              <Thumb card={e.card} size={22} />
              <button type="button" class="name" title="The card's page" onclick={() => onopen(e)}>{e.card.name}</button>
            </div>
          </td>
          <td class="c-set muted">{e.any_printing ? "any printing" : `${e.card.set.toUpperCase()} ${e.card.collector_number}`}</td>
          <td class="c-qty r"><TextCell kind="qty" label="Copies of {e.card.name} wanted" value={e.quantity}
            onchange={(v) => onedit([{ index: e.index, quantity: Number(v) }], `${e.card.name}: ${v} wanted`)} /></td>
          <td class="c-owned r num" class:done title="Owned of wanted">{have(e)} / {e.quantity}</td>
          <td class="c-finish"><ChoiceCell label="Finish wanted" options={finishes(e)} value={e.finish}
            onchange={(v, text) => onedit([{ index: e.index, finish: v }], `${e.card.name}: ${v ? text : "any finish"}`)} /></td>
          <td class="c-any"><input type="checkbox" aria-label="Any printing of {e.card.name} counts" checked={e.any_printing}
            onchange={(ev) => { const v = ev.currentTarget.checked; onedit([{ index: e.index, any_printing: v }], `${e.card.name}: ${v ? "any printing counts" : "this printing only"}`); }} /></td>
          <td class="c-price r num">{e.price !== null ? usd(e.price) : "—"}</td>
          <td class="c-value r num">{e.price !== null ? usd(e.price * e.quantity) : "—"}</td>
          <td class="c-acts">
            {#if !done}<button type="button" class="small" onclick={() => onadd(e)}>Add…</button>{/if}
            <button type="button" class="small ghost danger-text" aria-label="Remove {e.card.name} from the list" title="Remove from the list"
              onclick={() => onedit([{ index: e.index, remove: true }], `removed ${e.card.name}`)}>✕</button>
          </td>
        </tr>
      {/each}
      {#if entries.length > last}<tr class="spacer" aria-hidden="true"><td colspan="10" style:height="{(entries.length - last) * ROW_H}px"></td></tr>{/if}
      {#if !entries.length}<tr><td colspan="10" class="muted empty">No cards match.</td></tr>{/if}
    </tbody>
    <tfoot>
      <tr>
        <td class="c-tick"></td>
        <td class="c-name stick">{plural(entries.length, "card", "cards")}</td>
        <td class="c-set"></td>
        <td class="c-qty r num">{totals.wanted.toLocaleString()}</td>
        <td class="c-owned r num">{totals.owned.toLocaleString()}</td>
        <td class="c-finish"></td>
        <td class="c-any"></td>
        <td class="c-price"></td>
        <td class="c-value r num">{usd(totals.value)}</td>
        <td class="c-acts"></td>
      </tr>
    </tfoot>
  </table>
</div>

<style>
  .grid .c-owned { min-width: 70px; }
  /* Owned of wanted: amber until there are enough. */
  .grid tbody .c-owned { color: var(--amber); }
  .grid tbody .c-owned.done { color: var(--green); }
  .grid .c-any { min-width: 90px; text-align: center; }
  .grid .c-price { min-width: 84px; }
  .grid .c-finish { min-width: 90px; }
  @media (max-width: 800px) {
    .grid .c-any, .grid .c-price { display: none; }
  }
</style>
