<!-- The collection table: a row per card (its copies under it when opened), edited in
     place like a spreadsheet. Only the rows in view are drawn, so a collection of any
     size scrolls smoothly. -->
<script lang="ts">
  import { SvelteSet } from "svelte/reactivity";
  import { api, gameAPI } from "../lib/app.svelte";
  import { ask, openDialog } from "../lib/dialogs.svelte";
  import { fname, plural, usd } from "../lib/format";
  import type { Card } from "../lib/types";
  import CardRow from "./CardRow.svelte";
  import "./table.css";
  import SellDialog from "./SellDialog.svelte";
  import { columns, shown } from "./columns.svelte";
  import { forget, rowsOf, said, saveEdits, split, splits, type Edit, type Fields, type Row, type RowActions } from "./edits.svelte";

  let { cards = $bindable(), shows, sort, dir, selection, onsort, onfilter, onopen, openAll = false, compact = false }: {
    cards: Card[];
    /** Which of a card's rows the filters show (all when null). */
    shows: ((r: Row) => boolean) | null;
    sort: string;
    dir: "asc" | "desc";
    /** The ticked cards' keys (kept while filters hide them). */
    selection: SvelteSet<string>;
    onsort: (key: string) => void;
    onfilter: (name: "tags" | "lot", value: string) => void;
    onopen: (key: string) => void;
    /** Show every card's copies (the card page). */
    openAll?: boolean;
    /** Grow with the rows instead of scrolling (a few rows: the card page). */
    compact?: boolean;
  } = $props();

  // The cards whose copies are shown under them.
  const opened = new SvelteSet<string>();
  $effect(() => { if (openAll) for (const c of cards) opened.add(c.key); });

  const shownRows = (c: Card) => { const rs = rowsOf(c); return shows ? rs.filter(shows) : rs; };

  // ---- Drawing only the rows in view ----
  const CARD_H = 40, COPY_H = 34, OVERSCAN = 600;
  let scroller: HTMLDivElement;
  let scrollTop = $state(0);
  let viewH = $state(800);
  // Each card's top, from the heights of the cards before it.
  const tops = $derived.by(() => {
    void splits.version;
    const out = new Array<number>(cards.length + 1);
    let y = 0;
    cards.forEach((c, i) => { out[i] = y; y += CARD_H + (opened.has(c.key) ? shownRows(c).length * COPY_H : 0); });
    out[cards.length] = y;
    return out;
  });
  const first = $derived.by(() => { let lo = 0, hi = cards.length; const y = scrollTop - OVERSCAN; while (lo < hi) { const m = (lo + hi) >> 1; if (tops[m + 1] <= y) lo = m + 1; else hi = m; } return lo; });
  const last = $derived.by(() => { let i = first; const y = scrollTop + viewH + OVERSCAN; while (i < cards.length && tops[i] < y) i++; return i; });
  const visible = $derived(cards.slice(first, last));
  const width = $derived(4 + columns.filter((c) => c.on && c.key !== "set").length + (shown("set") ? 1 : 0));

  // ---- Totals of what the filters show ----
  const totals = $derived.by(() => {
    void splits.version;
    let copies = 0, value = 0;
    for (const c of cards) for (const r of shownRows(c)) { copies += r.quantity; value += r.market * r.quantity; }
    return { copies, value };
  });

  // ---- Editing ----
  function replace(old: Card, now: Card | null) {
    const i = cards.findIndex((c) => c.key === old.key);
    if (i < 0) return;
    if (now) cards[i] = now; else { cards.splice(i, 1); selection.delete(old.key); forget(old.key); }
  }

  /** The cards an edit on `card`'s row changes: every ticked card when it's one of
   *  several ticked, else just it. Ticked cards the filters hide are fetched. */
  async function targets(card: Card): Promise<Card[]> {
    if (!selection.has(card.key) || selection.size < 2) return [card];
    const listed = new Map(cards.map((c) => [c.key, c]));
    const out: Card[] = [];
    for (const key of selection) {
      const c = listed.get(key) || (await api<{ card: Card }>("GET", gameAPI(`/card?key=${encodeURIComponent(key)}`)).catch(() => null))?.card;
      if (c) out.push(c);
    }
    return out;
  }

  function actions(card: Card): RowActions {
    return {
      async edit(row, field, value, label) {
        const apply = (r: Row) => ({ [field]: typeof value === "function" ? value(r[field]) : value }) as Partial<Fields>;
        let edits: Edit[];
        if (row) {
          edits = [{ card, changed: { [row.id]: apply(row) } }];
        } else {
          // Price paid and date belong to the lot for copies bought in one.
          const fits = (r: Row) => !((field === "price_paid" || field === "acquired_date") && r.lot);
          edits = (await targets(card)).map((c) => ({ card: c, changed: Object.fromEntries(shownRows(c).filter(fits).map((r) => [r.id, apply(r)])) }))
            .filter((e) => Object.keys(e.changed).length);
        }
        const copies = edits.reduce((n, e) => n + rowsOf(e.card).filter((r) => r.id in e.changed).reduce((m, r) => m + r.quantity, 0), 0);
        await saveEdits(edits, `Saved: ${label} on ${said(edits, copies)}.`, replace);
      },
      split: (row) => split(card, row),
      async sell(row) {
        const sold = await openDialog<boolean>(SellDialog, { card, row });
        if (!sold) return;
        if (row.group) row.group.qty = 0;
        const now = (await api<{ card: Card }>("GET", gameAPI(`/card?key=${encodeURIComponent(card.key)}`)).catch(() => null))?.card || null;
        replace(card, now);
      },
      async remove(row) {
        if (row.group) { row.group.qty = 0; splits.version++; return; }
        if (!(await ask(`Delete ${plural(row.quantity, "copy", "copies")} of ${card.name} (${fname(row.finish)}, ${row.condition})?`))) return;
        await saveEdits([{ card, changed: { [row.id]: { quantity: 0 } } }], `Deleted ${plural(row.quantity, "copy", "copies")} of ${card.name}.`, replace);
      },
      filter: onfilter,
      open: () => onopen(card.key),
    };
  }

  // ↑ and ↓ in a typed cell move to the same column in the row above or below.
  function arrows(e: KeyboardEvent) {
    const input = e.target as HTMLInputElement;
    if (!(e.key === "ArrowUp" || e.key === "ArrowDown") || !input.classList?.contains("cell") || input.tagName !== "INPUT" || input.type === "number") return;
    const td = input.closest("td")!, tr = td.closest("tr")!;
    const col = [...tr.children].indexOf(td);
    let next: Element | null = tr;
    do { next = e.key === "ArrowDown" ? next.nextElementSibling : next.previousElementSibling; }
    while (next && !next.children[col]?.querySelector("input.cell"));
    const target = next?.children[col]?.querySelector<HTMLInputElement>("input.cell");
    if (target) { e.preventDefault(); target.focus(); }
  }

  const allTicked = $derived(cards.length > 0 && cards.every((c) => selection.has(c.key)));
  function tickAll() {
    if (allTicked) for (const c of cards) selection.delete(c.key);
    else for (const c of cards) selection.add(c.key);
  }

  /** Scrolls the table back to the top (the floating button). */
  export function toTop() { scroller.scrollTo({ top: 0, behavior: "smooth" }); }
</script>

{#snippet head(key: string, label: string, cls = "")}
  {@const col = columns.find((c) => c.key === key)}
  {#if col?.sort || key === "name"}
    {@const by = col?.sort || "name"}
    <th class={cls} class:sorted={sort === by} aria-sort={sort === by ? (dir === "asc" ? "ascending" : "descending") : undefined}>
      <button type="button" class="sorter" onclick={() => onsort(by)}>{label}{sort === by ? (dir === "asc" ? " ▲" : " ▼") : ""}</button>
    </th>
  {:else}
    <th class={cls}>{label}</th>
  {/if}
{/snippet}

<div class="table-scroll" class:compact bind:this={scroller} bind:clientHeight={viewH} onscroll={() => { scrollTop = scroller.scrollTop; }}>
  <table class="grid">
    <thead>
      <tr>
        <th class="c-tick"><input type="checkbox" aria-label="Select every card listed" checked={allTicked} onchange={tickAll} /></th>
        {@render head("name", "Name", "c-name stick")}
        {#if shown("set")}{@render head("set", "Set", "c-set")}{/if}
        {#if shown("qty")}{@render head("qty", "Qty", "c-qty r")}{/if}
        {#if shown("finish")}<th class="c-finish">Finish</th>{/if}
        {#if shown("condition")}<th class="c-condition">Cond.</th>{/if}
        {#if shown("language")}<th class="c-language">Lang.</th>{/if}
        {#if shown("lot")}<th class="c-lot">Purchase</th>{/if}
        {#if shown("paid")}{@render head("paid", "Paid each", "c-paid r")}{/if}
        {#if shown("acquired")}{@render head("acquired", "Acquired", "c-acquired")}{/if}
        {#if shown("tags")}{@render head("tags", "Tags", "c-tags")}{/if}
        {@render head("value", "Value", "c-value r")}
        <th class="c-acts"><span class="sr">Actions</span></th>
      </tr>
    </thead>
    <!-- The keys come from the cells' inputs, bubbling up. -->
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <tbody onkeydown={arrows}>
      {#if tops[first] > 0}<tr class="spacer" aria-hidden="true"><td colspan={width} style:height="{tops[first]}px"></td></tr>{/if}
      {#each visible as card (card.key)}
        {@const rows = shownRows(card)}
        <CardRow {card} {rows} hidden={card.copy_count - rows.reduce((n, r) => n + r.quantity, 0)} open={opened.has(card.key)} selected={selection.has(card.key)}
          ontoggle={() => (opened.has(card.key) ? opened.delete(card.key) : opened.add(card.key))}
          onselect={() => (selection.has(card.key) ? selection.delete(card.key) : selection.add(card.key))}
          act={actions(card)} />
      {/each}
      {#if tops[cards.length] - tops[last] > 0}<tr class="spacer" aria-hidden="true"><td colspan={width} style:height="{tops[cards.length] - tops[last]}px"></td></tr>{/if}
      {#if !cards.length}<tr><td colspan={width} class="muted empty">No cards match.</td></tr>{/if}
    </tbody>
    <tfoot>
      <tr>
        <td class="c-tick"></td>
        <td class="c-name stick">{plural(cards.length, "card", "cards")}</td>
        {#if shown("set")}<td class="c-set"></td>{/if}
        {#if shown("qty")}<td class="c-qty r num">{totals.copies.toLocaleString()}</td>{/if}
        {#each columns.filter((c) => c.on && !["set", "qty", "value"].includes(c.key)) as c (c.key)}<td class="c-{c.key}"></td>{/each}
        <td class="c-value r num">{usd(totals.value)}</td>
        <td class="c-acts"></td>
      </tr>
    </tfoot>
  </table>
</div>
