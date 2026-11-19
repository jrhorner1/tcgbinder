<!-- Lists and decks: every list, or (with ?id=) one list with what's owned and what the
     rest would cost. -->
<script lang="ts">
  import { api, app, failed, gameAPI, reload, status, whileRunning } from "../lib/app.svelte";
  import { askName, confirmDialog, openDialog } from "../lib/dialogs.svelte";
  import { fname, plural, sn, usd } from "../lib/format";
  import { go } from "../lib/router.svelte";
  import type { CatalogCard, ListSummary } from "../lib/types";
  import { openAdd } from "../add/add";
  import AskText from "../components/AskText.svelte";
  import Thumb from "../components/Thumb.svelte";
  import Tile from "../components/Tile.svelte";

  let { params }: { params: URLSearchParams } = $props();

  interface Entry { index: number; card: CatalogCard; quantity: number; any_printing: boolean; finish: string; owned: number; price: number | null }
  const id = $derived(params.get("id"));
  const all = $derived.by(() => { void app.reloads; return id ? null : api<ListSummary[]>("GET", gameAPI("/lists")); });
  const one = $derived.by(() => { void app.reloads; return id ? api<{ list: ListSummary; entries: Entry[] }>("GET", gameAPI(`/lists?id=${encodeURIComponent(id)}`)) : null; });

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
  function saveEntries(entries: Entry[], change: (next: (Omit<Entry, "index" | "owned" | "price"> | null)[]) => void, done?: string) {
    const next: (Omit<Entry, "index" | "owned" | "price"> | null)[] = entries.map((e) => ({ card: e.card, quantity: e.quantity, any_printing: e.any_printing, finish: e.finish }));
    change(next);
    put({ entries: next.filter(Boolean) }, done);
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
{:else if one}
  {#await one}
    <p class="muted">Loading…</p>
  {:then { list: l, entries }}
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
    {#if entries.length}
      <div class="card-box">
        {#each entries as e, i (e.index)}
          {@const have = Math.min(e.owned, e.quantity)}
          <div class="list-entry" class:have={have >= e.quantity}>
            <Thumb card={e.card} />
            <div class="text">
              <span class="name">{e.card.name}</span>
              <span class="meta">{[e.any_printing ? "any printing" : sn(e.card), e.finish ? fname(e.finish) : "", e.price !== null ? `${usd(e.price)} each` : "no price"].filter(Boolean).join(" · ")}</span>
              <label class="check small"><input type="checkbox" checked={e.any_printing} onchange={(ev) => { const v = ev.currentTarget.checked; saveEntries(entries, (n) => { n[i]!.any_printing = v; }); }} />Any printing</label>
            </div>
            <div class="entry-controls">
              <span class="owned-count" title="Owned of wanted">{have} / {e.quantity}</span>
              <input type="number" min="1" step="1" value={e.quantity} aria-label="Copies of {e.card.name} wanted"
                onchange={(ev) => { const q = parseInt(ev.currentTarget.value, 10); if (q >= 1) saveEntries(entries, (n) => { n[i]!.quantity = q; }); }} />
              {#if have < e.quantity}
                <button type="button" onclick={() => openAdd({ query: e.card.name, set: e.card.set })}>Add…</button>
              {:else}
                <span class="done">✓</span>
              {/if}
              <button type="button" aria-label="Remove {e.card.name} from the list" title="Remove from the list"
                onclick={() => saveEntries(entries, (n) => { n[i] = null; }, `Removed ${e.card.name} from ${l.name}.`)}>✕</button>
            </div>
          </div>
        {/each}
      </div>
    {:else}
      <p class="muted">No cards yet. Add some from a card's page (Add to list…), from a set's missing cards, or with Paste a list…</p>
    {/if}
  {:catch e}<p class="error">{e.message}</p>{/await}
{/if}

<style>
  .list-notes { white-space: pre-wrap; }
  .list-entry label.small { font-size: 12px; color: var(--muted); }
  /* Owned of wanted: amber until there are enough. */
  .owned-count { font-variant-numeric: tabular-nums; white-space: nowrap; color: var(--amber); }
  .list-entry.have .owned-count, .done { color: var(--green); }
  .done { min-width: 3.4em; text-align: center; }
</style>
