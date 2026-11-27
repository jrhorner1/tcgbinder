<!-- Statistics: the collection by price band, rarity, finish, condition, language,
     color and more, by month acquired, and the cards you have most copies of. Click a
     row to see those cards in the collection. -->
<script lang="ts">
  import { api, app, gameAPI } from "../lib/app.svelte";
  import { choiceNames, fname, languageName, plural, title, usd } from "../lib/format";
  import { go } from "../lib/router.svelte";
  import type { Card, TagTotal } from "../lib/types";
  import Bars from "../components/Bars.svelte";
  import CardRows from "../components/CardRows.svelte";

  interface Breakdown { id: string; name: string; param: string | null; rows: { key: string; cards: number; unique: number; value: number; paid: number }[] }
  interface Band { label: string; min: number | null; max: number | null; cards: number; value: number; share: number }
  interface Stats {
    breakdowns: Breakdown[]; bands: Band[]; byMonth: { month: string; cards: number; paid: number }[]; undated: number; mostCopies: Card[];
    tags: { tags: TagTotal[]; untagged: Omit<TagTotal, "name"> | null };
  }
  const data = $derived.by(() => { void app.reloads; return api<Stats>("GET", gameAPI("/stats")); });
  let allMonths = $state(false);

  function label(id: string, key: string) {
    switch (id) {
      case "rarity": return title(key || "unknown");
      case "finish": return fname(key);
      case "condition": return choiceNames.default_condition[key] || key;
      case "language": return languageName(key);
      case "Color": return ({ W: "White", U: "Blue", B: "Black", R: "Red", G: "Green" } as Record<string, string>)[key] || key;
      default: return key;
    }
  }
  const show = (p: Record<string, string>) => () => go("collection", p);
  function band(b: Band) {
    if (b.label === "No price") return show({ unpriced: "1" });
    const p: Record<string, string> = {};
    if (b.min !== null) p.price_min = String(b.min);
    if (b.max !== null) p.price_max = (b.max - 0.01).toFixed(2);
    return show(p);
  }
</script>

<h1>Statistics</h1>
<p class="muted">Counts are copies. Each bar's figures are copies and market value (and, by price, the share of the collection's value). Click a row to see those cards in the collection.</p>
{#await data}
  <p class="muted">Loading…</p>
{:then s}
  {@const months = [...s.byMonth].reverse()}
  <h2>By Price of One Copy</h2>
  <div class="card-box">
    <Bars max={Math.max(0.01, ...s.bands.map((b) => b.value))} color="var(--amber)"
      items={s.bands.map((b) => ({ label: b.label, value: b.value, text: `${b.cards.toLocaleString()} · ${usd(b.value)} · ${Math.round(b.share * 100)}%`, onclick: band(b) }))} />
  </div>
  <div class="two-col stats">
    {#each s.breakdowns as t (t.id)}
      <div class="card-box">
        <h2 style="margin-top:0">{t.name}</h2>
        {#if t.rows.length}
          <Bars max={Math.max(1, ...t.rows.map((r) => r.cards))} color="var(--blue)"
            items={t.rows.map((r) => ({ label: label(t.id, r.key), value: r.cards, text: `${r.cards.toLocaleString()} · ${usd(r.value)}`, onclick: t.param ? show({ [t.param]: r.key }) : null }))} />
        {:else}<p class="muted">No cards.</p>{/if}
      </div>
    {/each}
    {#if s.tags.tags.length}
      {@const rows = [...s.tags.tags.map((x) => ({ label: x.name, value: x.copies, text: `${x.copies.toLocaleString()} · ${usd(x.value)}`, onclick: show({ tags: x.name }) })),
        ...(s.tags.untagged ? [{ label: "Untagged", value: s.tags.untagged.copies, text: `${s.tags.untagged.copies.toLocaleString()} · ${usd(s.tags.untagged.value)}`, onclick: show({ untagged: "1" }) }] : [])]}
      <div class="card-box">
        <h2 style="margin-top:0">Tags</h2>
        <Bars items={rows} max={Math.max(1, ...rows.map((r) => r.value))} color="var(--blue)" />
        <p class="note">Copies carrying each tag, and their market value. A copy with two tags counts under both.</p>
      </div>
    {/if}
  </div>
  <div class="two-col">
    <div>
      <h2>By Month Acquired</h2>
      {#if months.length}
        <div class="card-box wide">
          <table class="months">
            <thead><tr><th>Month</th><th class="n">Copies</th><th class="n">Spent</th></tr></thead>
            <tbody>{#each allMonths ? months : months.slice(0, 12) as m (m.month)}<tr><td>{m.month}</td><td class="n">{m.cards.toLocaleString()}</td><td class="n">{usd(m.paid)}</td></tr>{/each}</tbody>
          </table>
          {#if !allMonths && months.length > 12}<button type="button" class="link" onclick={() => (allMonths = true)}>Show all {months.length} months</button>{/if}
        </div>
      {:else}<p class="muted">No acquired dates recorded.</p>{/if}
      {#if s.undated}<p class="note">{plural(s.undated, "copy has", "copies have")} no acquired date and aren't counted here.</p>{/if}
    </div>
    <div>
      <h2>Most Copies</h2>
      {#if s.mostCopies.length}<div class="card-box"><CardRows cards={s.mostCopies} /></div>{:else}<p class="muted">No card has more than one copy.</p>{/if}
    </div>
  </div>
{:catch e}<p class="error">{e.message}</p>{/await}

<style>
  .stats { margin-top: 16px; gap: 16px 28px; }
  .wide { overflow-x: auto; }
  table.months { width: 100%; border-collapse: collapse; font-variant-numeric: tabular-nums; }
  .months th, .months td { text-align: left; padding: 5px 6px; border-bottom: 1px solid var(--line); white-space: nowrap; }
  .months th { font-size: 12px; color: var(--muted); font-weight: 600; }
  .months .n { text-align: right; }
  .months tr:last-child td { border-bottom: 0; }
</style>
