<!-- Lists and decks: every list, or (with ?id=) one list with what's owned and what the
     rest would cost, as a table with the collection's filters (in the address, like
     the collection's). -->
<script lang="ts" module>
  import { SvelteSet } from "svelte/reactivity";
  // Ticked entries (by index), for the list they were ticked on.
  const ticked = { list: "", set: new SvelteSet<number>() };
</script>

<script lang="ts">
  import { api, app, failed, gameAPI, reload, status, whileRunning } from "../lib/app.svelte";
  import { askName, confirmDialog, openDialog } from "../lib/dialogs.svelte";
  import { plural, usd } from "../lib/format";
  import { go } from "../lib/router.svelte";
  import type { ListSummary } from "../lib/types";
  import { openAdd } from "../add/add";
  import AskText from "../components/AskText.svelte";
  import Tile from "../components/Tile.svelte";
  import Filters from "../collection/Filters.svelte";
  import ListTable, { type Entry, type EntryChange } from "../lists/ListTable.svelte";

  let { params }: { params: URLSearchParams } = $props();

  interface OneList { list: ListSummary; sets: { Code: string; Label: string }[]; entries: Entry[] }
  const id = $derived(params.get("id"));
  const all = $derived.by(() => { void app.reloads; return id ? null : api<ListSummary[]>("GET", gameAPI("/lists")); });

  // One list: loaded whenever the filters or the data change; the last one stays on
  // screen meanwhile, so the table keeps its place.
  let one = $state<OneList | null>(null);
  let error = $state("");
  const query = $derived(params.toString());
  const filtering = $derived([...params.keys()].some((k) => !["id", "sort", "dir"].includes(k)));
  $effect(() => {
    void app.reloads;
    const q = query, listId = id;
    if (!listId) { one = null; return; }
    if (ticked.list !== listId) { ticked.list = listId; ticked.set.clear(); }
    (async () => {
      try {
        const d = await api<OneList>("GET", gameAPI(`/lists?${q}`));
        if (q !== query) return; // the filters changed meanwhile
        one = d; error = "";
      } catch (e) { error = e instanceof Error ? e.message : String(e); }
    })();
  });
  const selection = ticked.set;
  const nav = (mutate: (p: URLSearchParams) => void, replace = false) => { const p = new URLSearchParams(params); mutate(p); go("lists", p, replace); };
  const clearFilters = () => go("lists", Object.fromEntries([...params].filter(([k]) => ["id", "sort", "dir"].includes(k))));
  function sortBy(by: string) {
    nav((p) => {
      if (p.get("sort") === by) p.set("dir", p.get("dir") === "desc" ? "asc" : "desc");
      else { p.set("sort", by); p.set("dir", ["name", "set"].includes(by) ? "asc" : "desc"); }
    }, true);
  }

  /** Saves changes to entries (by index), with Undo. */
  async function edit(changes: EntryChange[], label: string) {
    const entries = one?.entries || [];
    const byIndex = new Map(entries.map((e) => [e.index, e]));
    const removed = changes.filter((c) => c.remove).map((c) => byIndex.get(c.index)!).filter(Boolean);
    const restore: EntryChange[] = changes.filter((c) => !c.remove).map((c) => {
      const e = byIndex.get(c.index)!;
      return { index: c.index, quantity: e.quantity, any_printing: e.any_printing, finish: e.finish };
    });
    try {
      await api("PUT", gameAPI(`/lists/entries?id=${encodeURIComponent(id!)}`), { changes });
      if (removed.length) selection.clear(); // the places after them moved up
      status(`Saved: ${label}.`, false, async () => {
        if (restore.length) await api("PUT", gameAPI(`/lists/entries?id=${encodeURIComponent(id!)}`), { changes: restore });
        for (const e of removed) await api("POST", gameAPI(`/lists/entries?id=${encodeURIComponent(id!)}`), { card: e.card, quantity: e.quantity, any_printing: e.any_printing, finish: e.finish });
      });
      reload();
    } catch (e) { failed(e); }
  }
  async function removeTicked() {
    if (!(await confirmDialog(`Remove ${plural(selection.size, "card", "cards")} from the list? The cards in your collection stay.`, { danger: true, action: "Remove" }))) return;
    edit([...selection].map((index) => ({ index, remove: true })), `removed ${plural(selection.size, "card", "cards")}`);
  }

  async function create() {
    const name = await askName("New list", "");
    if (!name) return;
    try { const l = await api("POST", gameAPI("/lists"), { name }); go("lists", { id: l.id }); } catch (e) { failed(e); }
  }

  // ---- One list ----
  const path = () => gameAPI(`/lists?id=${encodeURIComponent(id!)}`);
  async function put(body: Record<string, unknown>, done?: string) {
    try { await api("PUT", path(), body); if (done) status(done); reload(); } catch (e) { failed(e); }
  }
  async function rename(l: ListSummary) { const name = await askName("Rename the list", l.name); if (name) put({ name }); }
  async function notes(l: ListSummary) { const v = await openDialog<string>(AskText, { title: "Notes", value: l.notes || "", rows: 4 }); if (v !== null) put({ notes: v }); }
  async function remove(l: ListSummary) {
    if (!(await confirmDialog(`Delete the list ${l.name}? The cards in your collection stay.`, { danger: true, action: "Delete" }))) return;
    try { await api("DELETE", path()); status(`Deleted ${l.name}.`); go("lists"); } catch (e) { failed(e); }
  }
  async function paste(l: ListSummary) {
    const text = await openDialog<string>(AskText, {
      title: "Paste a card list", rows: 10, action: "Add to list", placeholder: "4 Lightning Bolt (M11) 149\n2 Opt\n1 Shock *F*",
      message: "One card per line, as Moxfield, Archidekt or MTG Arena export them. A line with a set (and number) wants that printing; a line with just a name counts any printing as owned.",
    });
    if (!text?.trim()) return;
    try {
      const r = await whileRunning(api("POST", gameAPI(`/lists/import?id=${encodeURIComponent(id!)}`), new Blob([text])));
      status(`Added ${plural(r.added, "card", "cards")} to ${l.name}.${r.unmatched.length ? ` Not found: ${r.unmatched.join("; ")}` : ""}`, r.unmatched.length > 0);
      reload();
    } catch (e) { failed(e); }
  }
</script>

{#if all}
  <div class="section-head"><h1>Lists</h1><button type="button" class="primary" onclick={create}>New list</button></div>
  <p class="muted">Decks and want lists. Each shows how many of its cards you own and what the rest would cost. Add cards from a card's page, from a set's missing cards, or paste a decklist.</p>
  {#await all then lists}
    {#if lists.length}
      <div class="card-box"><div class="rows">
        {#each lists as l (l.id)}
          <a class="row" href="#/lists?id={encodeURIComponent(l.id)}">
            <span class="name">{l.name}</span>
            <span class="where">{l.owned} of {plural(l.cards, "card", "cards")} owned{l.missing ? ` · ${usd(l.missingCost)} to complete` : " · complete"}</span>
            <span class="amt">{usd(l.marketValue)}</span>
          </a>
        {/each}
      </div></div>
    {:else}
      <p class="muted">No lists yet.</p>
    {/if}
  {:catch e}<p class="error">{e.message}</p>{/await}
{:else if id}
  {#if error}<p class="error">{error}</p>{/if}
  {#if one}
    {@const l = one.list}
    <p><a href="#/lists">← Lists</a></p>
    <div class="section-head">
      <h1>{l.name}</h1>
      <div class="buttons">
        <button type="button" onclick={() => rename(l)}>Rename</button>
        <button type="button" onclick={() => notes(l)}>Notes</button>
        <button type="button" onclick={() => paste(l)}>Paste a list…</button>
        <a class="button" href={gameAPI(`/lists/export?id=${encodeURIComponent(l.id)}`)}>Export</a>
        <button type="button" class="danger" onclick={() => remove(l)}>Delete</button>
      </div>
    </div>
    {#if l.notes}<p class="list-notes">{l.notes}</p>{/if}
    <p><label class="check"><input type="checkbox" checked={l.remove_owned}
      onchange={(e) => put({ remove_owned: e.currentTarget.checked }, e.currentTarget.checked ? `Cards you own will leave ${l.name}.` : `Cards you own will stay on ${l.name}.`)} />
      Take cards off this list once I own them</label></p>
    <div class="tiles">
      <Tile cap="CARDS" value={l.cards.toLocaleString()} detail={plural(l.entries, "entry", "entries")} />
      <Tile cap="OWNED" value={String(l.owned)} detail={l.cards ? `${Math.round((l.owned / l.cards) * 100)}% of the list` : ""} />
      <Tile cap="TO COMPLETE" value={usd(l.missingCost)} detail={l.missing ? `${plural(l.missing, "card", "cards")} missing, at market price` : "you own every card"} color="var(--blue)" />
      <Tile cap="LIST VALUE" value={usd(l.marketValue)} detail="every card, at market price" />
    </div>
    {#if l.entries}
      <div class="one-list">
        {#key app.game}<Filters q={params} {nav} kind="list" sets={one.sets} />{/key}
        <div class="counts muted">
          <span>{filtering ? `Matching: ${plural(one.entries.length, "card", "cards")} of ${l.entries}` : plural(l.entries, "card", "cards")}</span>
          {#if filtering}<button type="button" class="link" onclick={clearFilters}>Clear filters</button>{/if}
        </div>
        <div class="selbar" class:ticked={selection.size > 0}>
          <span class="count">{selection.size ? `${plural(selection.size, "card", "cards")} ticked` : "Click a cell to change it · tick cards to remove them together"}</span>
          <button type="button" disabled={!one.entries.length} onclick={() => one!.entries.forEach((e) => selection.add(e.index))}>Select all</button>
          <button type="button" disabled={!selection.size} onclick={() => selection.clear()}>Clear</button>
          <button type="button" class="danger-text" disabled={!selection.size} onclick={removeTicked}>Remove…</button>
        </div>
        <ListTable entries={one.entries} sort={params.get("sort") || ""} dir={params.get("dir") || "asc"} {selection} onsort={sortBy} onedit={edit}
          onopen={(e) => go("collection", { key: e.card.id })} onadd={(e) => openAdd({ query: e.card.name, set: e.any_printing ? "" : e.card.set })} />
      </div>
    {:else}
      <p class="muted">No cards yet. Add some from a card's page (Add to list…), from a set's missing cards, or with Paste a list…</p>
    {/if}
  {:else if !error}
    <p class="muted">Loading…</p>
  {/if}
{/if}

<style>
  .list-notes { white-space: pre-wrap; }
  .one-list { display: flex; flex-direction: column; gap: 8px; }
  .counts { font-size: 13px; display: flex; gap: 12px; align-items: center; justify-content: space-between; }
  .selbar { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; background: var(--panel); border: 1px solid var(--line); border-radius: var(--radius); padding: 8px 10px; }
  .selbar .count { flex: 1; min-width: 9em; font-size: 14px; }
  .selbar.ticked { border-color: var(--blue); }
</style>
