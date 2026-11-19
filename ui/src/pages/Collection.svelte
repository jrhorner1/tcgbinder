<!-- The Collection page: unfinished lots, a lot's or set's panel when filtered to one,
     the filters, and the table of cards (or, with ?key=, one card's page). -->
<script lang="ts" module>
  import { SvelteSet } from "svelte/reactivity";
  // Ticked cards, per game. Ticks stay while filters hide the cards, so a selection
  // can be built from several searches.
  const ticks = new Map<string, SvelteSet<string>>();
</script>

<script lang="ts">
  import { api, app, exportCollection, failed, gameAPI, loadFilters, status } from "../lib/app.svelte";
  import { ask, askDialog } from "../lib/dialogs.svelte";
  import { plural, usd } from "../lib/format";
  import { lotProgress } from "../lib/lots";
  import { go } from "../lib/router.svelte";
  import { sameTag } from "../lib/tags";
  import type { Card, CardList, TagTotal } from "../lib/types";
  import { openAdd } from "../add/add";
  import CardPage from "../card/CardPage.svelte";
  import Filters from "../collection/Filters.svelte";
  import LotPanel from "../collection/LotPanel.svelte";
  import SetPanel from "../collection/SetPanel.svelte";
  import Table from "../collection/Table.svelte";
  import { columns, toggle } from "../collection/columns.svelte";
  import type { Row } from "../collection/edits.svelte";

  let { params }: { params: URLSearchParams } = $props();

  // Sort keys that start ascending when chosen; the rest start with the most first.
  const ASCENDING_FIRST = ["name", "number", "tag", "set"];
  const SORTS: [string, string][] = [["name", "Name"], ["set", "Set"], ["number", "Number"], ["price", "Price each"], ["value", "Value"],
    ["gain", "Gain / loss"], ["paid", "Price paid"], ["quantity", "Quantity"], ["acquired", "Acquired"], ["released", "Released"], ["rarity", "Rarity"], ["tag", "Tag"]];

  if (!ticks.has(app.game)) ticks.set(app.game, new SvelteSet());
  const selection = ticks.get(app.game)!;

  const key = $derived(params.get("key"));
  // GET /cards's parameters: the address's, less the card shown.
  const query = $derived.by(() => { const p = new URLSearchParams(params); p.delete("key"); return p.toString(); });
  const filtering = $derived([...new URLSearchParams(query).keys()].some((k) => k !== "sort" && k !== "dir"));

  let data = $state<CardList | null>(null);
  let cards = $state<Card[]>([]);
  let tagTotals = $state<TagTotal[]>([]);
  let error = $state("");
  let showColumns = $state(false);
  let table = $state<ReturnType<typeof Table>>();
  let scrolled = $state(false);
  // The table fills the window under the selection bar (which sticks under the top bar).
  let selbarHeight = $state(50);

  // Load the cards whenever the filters change or the data changes elsewhere.
  $effect(() => {
    if (key) return;
    void app.reloads;
    const q = query;
    (async () => {
      try {
        const [d, t] = await Promise.all([api<CardList>("GET", gameAPI(`/cards?${q}`)),
          app.filters?.tags.length ? api<{ tags: TagTotal[] }>("GET", gameAPI("/tags")) : { tags: [] }]);
        if (q !== query) return; // the filters changed meanwhile
        data = d; cards = d.cards; tagTotals = t.tags; error = "";
      } catch (e) { error = e instanceof Error ? e.message : String(e); }
    })();
  });

  const nav = (mutate: (p: URLSearchParams) => void, replace = false) => {
    const p = new URLSearchParams(params);
    p.delete("key"); // a change of filters shows the list
    mutate(p);
    go("collection", p, replace);
  };

  // Which of a card's rows the filters show: a lot's filter shows the copies bought in
  // it, a tag's the copies carrying it, and so on (null when no such filter is set).
  const shows = $derived.by((): ((r: Row) => boolean) | null => {
    const tags = (params.get("tags") || "").split(",").map((t) => t.trim()).filter(Boolean);
    const not = (params.get("tags_not") || "").split(",").map((t) => t.trim()).filter(Boolean);
    const lot = params.get("lot") || "", untagged = params.get("untagged") === "1";
    if (!lot && !tags.length && !not.length && !untagged) return null;
    return (r) => {
      const has = (t: string) => r.tags.some((x) => sameTag(x, t));
      return (!lot || (lot === "none" ? !r.lot : r.lot === lot)) && (!tags.length || tags.some(has)) && !not.some(has) && (!untagged || !r.tags.length);
    };
  });

  // Clicking a tag adds it to the tags chosen; a lot replaces the lot.
  function filterBy(name: "tags" | "lot", value: string) {
    nav((p) => {
      if (name === "lot") { p.set("lot", value); return; }
      const tags = (p.get("tags") || "").split(",").map((t) => t.trim()).filter(Boolean);
      if (!tags.some((t) => sameTag(t, value))) p.set("tags", [...tags, value].join(","));
    });
  }

  function sortBy(by: string) {
    nav((p) => {
      const same = (data?.sort || "") === by;
      p.set("sort", by);
      p.set("dir", same ? (data?.dir === "asc" ? "desc" : "asc") : ASCENDING_FIRST.includes(by) ? "asc" : "desc");
    }, true);
  }

  async function exportListed() {
    const format = await askDialog<"csv" | "report">("Export these cards", [
      `The ${plural(cards.length, "card", "cards")} the list shows, in its order.`,
      "CSV opens in spreadsheets and other collection apps, and Import here reads it back. The valuation report is a page to print or save as PDF, with every card's value by set.",
    ], [{ label: "Valuation report", value: "report" }, { label: "CSV", value: "csv", primary: true }]);
    if (format) exportCollection(format, new URLSearchParams(query));
  }

  async function deleteTicked() {
    if (!(await ask(`Delete ${plural(selection.size, "card", "cards")} (every copy) from your collection? A backup is taken first, so this can be undone.`))) return;
    try {
      const r = await api("POST", gameAPI("/bulk"), { keys: [...selection], delete: true });
      status(`Deleted ${plural(r.Deleted, "card", "cards")}.`, false, r.undo);
      cards = cards.filter((c) => !selection.has(c.key));
      selection.clear();
      await loadFilters();
    } catch (e) { failed(e); }
  }

  const hiddenTicks = $derived.by(() => { const listed = new Set(cards.map((c) => c.key)); return [...selection].filter((k) => !listed.has(k)).length; });
  const lot = $derived(app.lots.find((l) => l.id === params.get("lot")));
  const banners = $derived(app.lots.filter((l) => l.open && l.id !== params.get("lot")));
  const clearFilters = () => go("collection", Object.fromEntries([...params].filter(([k]) => k === "sort" || k === "dir")));
</script>

<svelte:window onscroll={() => { scrolled = window.scrollY > 400; }} />

{#if key}
  <CardPage {key} />
{:else}
  <datalist id="tag-names">{#each app.filters?.tags || [] as t}<option value={t}></option>{/each}</datalist>

  {#each banners as l (l.id)}
    <div class="card-box notice lot-banner">
      <div class="grow">
        <strong>📦 {l.name}</strong><span class="muted"> — {lotProgress(l)} · {usd(l.total_paid)} paid</span>
        {#if l.expected}<div class="bar"><i style:width="{Math.min(100, (l.entered / l.expected) * 100)}%"></i></div>{/if}
      </div>
      <button type="button" class="primary" onclick={() => openAdd({ lot: l.id })}>Keep adding</button>
      <button type="button" onclick={() => nav((p) => p.set("lot", l.id))}>Show its cards</button>
    </div>
  {/each}
  {#if lot}<LotPanel {lot} />{/if}
  {#if params.get("set")}<SetPanel code={params.get("set")!} />{/if}

  <div class="collection" style:--selbar-h="{selbarHeight}px">
    {#key app.game}<Filters q={params} {nav} {tagTotals} />{/key}

    {#if error}<p class="error">{error}</p>{/if}
    {#if data}
      <div class="counts muted">
        <span>
          {filtering ? "Matching: " : ""}{plural(data.uniqueCards, "unique card", "unique cards")} · {plural(data.cardCount, "card", "cards")} · {plural(data.sets, "set", "sets")}
          <span class="money-line"> · {usd(data.marketValue)} value · {usd(data.pricePaid)} paid</span>
        </span>
        <span class="count-actions">
          {#if filtering}<button type="button" class="link" onclick={clearFilters}>Clear filters</button>{/if}
          {#if cards.length}<button type="button" class="link" onclick={exportListed}>Export…</button>{/if}
        </span>
      </div>

      <div class="selbar" class:ticked={selection.size > 0} bind:clientHeight={selbarHeight}>
        <span class="count">
          {#if selection.size}
            {plural(selection.size, "card", "cards")} ticked{hiddenTicks ? ` (${hiddenTicks} hidden by filters)` : ""} — editing a ticked card's row changes them all
          {:else}
            Click a cell to change it · tick cards to change them together
          {/if}
        </span>
        <button type="button" disabled={!cards.length} onclick={() => cards.forEach((c) => selection.add(c.key))}>
          {cards.length > 1 ? `Select all ${cards.length.toLocaleString()}` : "Select all"}
        </button>
        <button type="button" disabled={!selection.size} onclick={() => selection.clear()}>Clear</button>
        <button type="button" class="danger-text" disabled={!selection.size} onclick={deleteTicked}>Delete…</button>
        <span class="grow"></span>
        <select aria-label="Sort by" value={data.sort} onchange={(e) => { const by = e.currentTarget.value; nav((p) => { p.set("sort", by); p.set("dir", ASCENDING_FIRST.includes(by) ? "asc" : "desc"); }, true); }}>
          {#each SORTS as [v, label]}<option value={v}>Sort: {label}</option>{/each}
        </select>
        <div class="columns-menu">
          <button type="button" aria-expanded={showColumns} onclick={() => (showColumns = !showColumns)}>Columns…</button>
          {#if showColumns}
            <div class="menu" role="group" aria-label="Columns shown">
              {#each columns.filter((c) => !c.always) as c (c.key)}
                <label class="check"><input type="checkbox" checked={c.on} onchange={() => toggle(c.key)} /> {c.label}</label>
              {/each}
            </div>
          {/if}
        </div>
      </div>

      <Table bind:this={table} bind:cards {shows} sort={data.sort} dir={data.dir} {selection} onsort={sortBy} onfilter={filterBy}
        onopen={(k) => nav((p) => p.set("key", k))} />
    {:else if !error}
      <p class="muted">Loading…</p>
    {/if}
  </div>
  {#if scrolled}
    <button type="button" class="to-top" title="Back to the top" aria-label="Back to the top" onclick={() => { window.scrollTo({ top: 0, behavior: "smooth" }); table?.toTop(); }}>↑</button>
  {/if}
{/if}

<style>
  .collection { display: flex; flex-direction: column; gap: 8px; }
  .counts { font-size: 13px; margin: 0; display: flex; flex-wrap: wrap; justify-content: space-between; gap: 8px; align-items: center; }
  .money-line { white-space: nowrap; }
  .count-actions { display: flex; gap: 12px; flex: none; }
  /* An unfinished lot: keep entering its cards from here. */
  .lot-banner { display: flex; gap: 12px; align-items: center; justify-content: space-between; flex-wrap: wrap; }
  .lot-banner .grow { flex: 1; min-width: 220px; }
  /* The selection bar sticks under the top bar as the page scrolls. */
  .selbar { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; background: var(--panel); border: 1px solid var(--line); border-radius: var(--radius); padding: 8px 10px;
    position: sticky; top: var(--topbar-h, 58px); z-index: 4; box-shadow: 0 6px 12px -8px rgb(0 0 0 / 50%); }
  .selbar .count { flex: 1; min-width: 9em; font-size: 14px; }
  .selbar.ticked { border-color: var(--blue); }
  /* Back to the top of the collection, once the list is scrolled. */
  .to-top { position: fixed; right: 20px; bottom: 20px; z-index: 6; width: 44px; height: 44px; border-radius: 50%; padding: 0; font-size: 20px;
    background: var(--blue); border-color: var(--blue); color: #fff; box-shadow: 0 6px 18px rgb(0 0 0 / 35%); }
  @media (max-width: 800px) {
    /* A phone's selection bar is too tall to stick: it scrolls away with the page. */
    .selbar { position: static; }
    .selbar .count { flex-basis: 100%; }
    .selbar button { flex: 1; }
    .to-top { right: 14px; bottom: calc(var(--tabbar) + env(safe-area-inset-bottom) + 14px); }
  }
  .selbar .grow { flex: 1; }
  .selbar select { max-width: 200px; }
  .columns-menu { position: relative; }
  .columns-menu .menu { position: absolute; right: 0; top: calc(100% + 4px); z-index: 20; background: var(--panel); border: 1px solid var(--line); border-radius: 10px;
    padding: 8px 12px; display: grid; gap: 6px; box-shadow: 0 8px 24px rgb(0 0 0 / 35%); white-space: nowrap; }
  .columns-menu label { display: flex; gap: 8px; align-items: center; }
</style>
