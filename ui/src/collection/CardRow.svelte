<!-- One card in the collection table: its row, and when opened (▸), a row for each
     group of its copies. Every copy cell is edited in place. On the card's row an edit
     changes all the copies shown; on a copy's row, just that group. -->
<script lang="ts">
  import { app, filters } from "../lib/app.svelte";
  import { fname, languageName, plural, usd } from "../lib/format";
  import type { Card } from "../lib/types";
  import Thumb from "../components/Thumb.svelte";
  import ChoiceCell from "./ChoiceCell.svelte";
  import TagsCell from "./TagsCell.svelte";
  import TextCell from "./TextCell.svelte";
  import { common, MIXED } from "./cells";
  import { shown } from "./columns.svelte";
  import type { Row, RowActions } from "./edits.svelte";

  let { card, rows, hidden, open, selected, ontoggle, onselect, act }: {
    card: Card;
    /** The rows of copies the filters show. */
    rows: Row[];
    /** Copies the filters hide. */
    hidden: number;
    open: boolean;
    selected: boolean;
    ontoggle: () => void;
    onselect: () => void;
    act: RowActions;
  } = $props();

  const finishes = $derived((card.finishes.length ? card.finishes : filters().finishes).map((f) => [f, fname(f)] as [string, string]));
  const conditions = $derived(filters().conditions.map((c) => [c, c] as [string, string]));
  const lots = $derived([["", "—"], ...app.lots.map((l) => [l.id, `📦 ${l.name}`])] as [string, string][]);
  const copies = $derived(rows.reduce((n, r) => n + r.quantity, 0));
  const value = $derived(rows.reduce((n, r) => n + r.market * r.quantity, 0));
  const priced = $derived(rows.some((r) => r.market > 0));
  // A card with one group of copies needs no opening: its row is that group.
  const single = $derived(rows.length === 1 ? rows[0] : null);
  const tagsOf = (rs: Row[]) => [...new Map(rs.flatMap((r) => r.tags).map((t) => [t.toLowerCase(), t])).values()].sort((a, b) => a.localeCompare(b));
  const lotName = (id: string) => app.lots.find((l) => l.id === id)?.name || "a lot";
</script>

{#snippet copyCells(rs: Row[], row: Row | null)}
  {@const one = row || (rs.length === 1 ? rs[0] : null)}
  {@const free = rs.filter((r) => !r.lot)}
  {#if shown("qty")}
    <td class="c-qty r">
      {#if one}
        <TextCell kind="qty" label="Quantity" value={one.quantity} onchange={(v) => act.edit(one, "quantity", v, `quantity ${v}`)} />
      {:else}
        <span class="cell readonly r" title="Open the card (▸) to change each group's quantity">{copies}</span>
      {/if}
    </td>
  {/if}
  {#if shown("finish")}
    <td class="c-finish"><ChoiceCell label="Finish" options={finishes} value={common(rs, "finish")} onchange={(v, t) => act.edit(row, "finish", v, `finish ${t}`)} /></td>
  {/if}
  {#if shown("condition")}
    <td class="c-condition"><ChoiceCell label="Condition" options={conditions} value={common(rs, "condition")} onchange={(v) => act.edit(row, "condition", v, `condition ${v}`)} /></td>
  {/if}
  {#if shown("language")}<td class="c-language muted">{row ? "" : languageName(card.lang)}</td>{/if}
  {#if shown("lot")}
    <td class="c-lot"><ChoiceCell label="Purchase" options={lots} value={common(rs, "lot")}
      onchange={(v) => act.edit(row, "lot", v, v ? `put in ${lotName(v)}` : "taken out of its lot")} /></td>
  {/if}
  {#if shown("paid")}
    <td class="c-paid">
      <TextCell kind="money" label="Price paid each" value={free.length ? common(free, "price_paid") : common(rs, "price_paid")} disabled={!free.length}
        title={!free.length ? "Set by the lot: its price is shared among its cards" : rs.length > free.length ? "Changes the copies not bought in a lot" : ""}
        onchange={(v) => act.edit(row, "price_paid", v, `price paid ${usd(Number(v))}`)} />
    </td>
  {/if}
  {#if shown("acquired")}
    <td class="c-acquired">
      <TextCell kind="date" label="Acquired" value={free.length ? common(free, "acquired_date") : common(rs, "acquired_date")} disabled={!free.length}
        title={!free.length ? "Set by the lot" : ""} onchange={(v) => act.edit(row, "acquired_date", v, `acquired ${v || "(no date)"}`)} />
    </td>
  {/if}
  {#if shown("tags")}
    {@const all = tagsOf(rs)}
    <td class="c-tags">
      <TagsCell tags={all} partial={row ? [] : all.filter((t) => !rs.every((r) => r.tags.some((x) => x.toLowerCase() === t.toLowerCase())))}
        onadd={(t) => act.edit(row, "tags", (old: string[]) => (old.some((x) => x.toLowerCase() === t.toLowerCase()) ? old : [...old, t]), `tag “${t}” added`)}
        onremove={(t) => act.edit(row, "tags", (old: string[]) => old.filter((x) => x.toLowerCase() !== t.toLowerCase()), `tag “${t}” removed`)}
        onpick={(t) => act.filter("tags", t)} />
    </td>
  {/if}
  <td class="c-value r num">
    {#if row}{row.market ? usd(row.market * row.quantity) : "—"}{:else}{priced ? usd(value) : "—"}{/if}
  </td>
  <td class="c-acts">
    {#if one}
      {#if one.quantity > 1}<button type="button" class="small" title="Move one copy to a group of its own; again, to move another into that group" onclick={() => act.split(one)}>Split</button>{/if}
      <button type="button" class="small ghost" onclick={() => act.sell(one)}>Sell…</button>
      <button type="button" class="small ghost danger-text" aria-label="Delete these copies" title="Delete these copies" onclick={() => act.remove(one)}>✕</button>
    {/if}
  </td>
{/snippet}

<tr class="card" class:selected>
  <td class="c-tick"><input type="checkbox" checked={selected} aria-label="Select {card.name}" onchange={onselect} /></td>
  <td class="c-name stick">
    <div class="namecell">
      <button type="button" class="caret" aria-label={open ? "Hide the copies" : "Show the copies"} aria-expanded={open} onclick={ontoggle}
        style:visibility={rows.length > 1 || open ? "visible" : "hidden"}>{open ? "▾" : "▸"}</button>
      <Thumb card={card} size={22} />
      <button type="button" class="name" title="The card's page: printing, price watch, selling" onclick={act.open}>{card.name}</button>
      {#if rows.length > 1}<span class="muted small">{rows.length} groups</span>{/if}
      {#if hidden}<span class="muted small" title="Copies the filters don't show">+{hidden}</span>{/if}
      {#if single?.group}<span class="muted small" title="Change something about it to keep it apart">split</span>{/if}
    </div>
  </td>
  {#if shown("set")}<td class="c-set muted">{card.set} {card.collector_number}</td>{/if}
  {@render copyCells(rows, null)}
</tr>
{#if open}
  {#each rows as r (r.id)}
    <tr class="copy" class:pending={!!r.group}>
      <td class="c-tick"></td>
      <td class="c-name stick"><div class="copyof">{plural(r.quantity, "copy", "copies")}{r.group ? " · split off: change something to keep it apart" : ""}</div></td>
      {#if shown("set")}<td class="c-set"></td>{/if}
      {@render copyCells([r], r)}
    </tr>
  {/each}
{/if}
