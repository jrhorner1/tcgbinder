<!-- The Add dialog's catalog search, by name and optionally set: results, each with
     how many, finish, condition and Add. After an Add the text is selected, so typing
     searches for the next card. -->
<script lang="ts" module>
  import { api, failed, filters, gameAPI } from "../lib/app.svelte";
  export interface Picked { quantity: number; finish: string; condition: string }
  interface CatalogSet { code: string; name: string; released: string }
  // The catalog's sets, loaded once for the set menu.
  let catalogSets: Promise<CatalogSet[]> | null = null;
  const loadSets = () => (catalogSets ??= api<{ sets: CatalogSet[] }>("GET", gameAPI("/catalog/sets")).then((r) => r.sets, () => { catalogSets = null; return []; }));
</script>

<script lang="ts">
  import { onMount } from "svelte";
  import { addDefaults } from "../lib/cards";
  import { fname, sn, title, usd } from "../lib/format";
  import type { CatalogCard } from "../lib/types";
  import Thumb from "../components/Thumb.svelte";

  let { query = "", set = "", found = null, onadd }: {
    query?: string;
    set?: string;
    /** Results to show at first, instead of searching (a scan's candidates). */
    found?: { cards: CatalogCard[]; owned: number[] } | null;
    onadd: (c: CatalogCard, a: Picked) => Promise<void>;
  } = $props();

  let text = $state(query);
  // The set menu: a set code, or a set's name as typed (matched to its code).
  let setText = $state(set.toUpperCase());
  let sets = $state<CatalogSet[]>([]);
  const setCode = $derived.by(() => {
    const t = setText.trim().toLowerCase();
    if (!t) return "";
    return sets.find((s) => s.code === t || s.name.toLowerCase() === t)?.code || t;
  });
  let cards = $state<{ card: CatalogCard; owned: number; qty: number; finish: string; condition: string; busy: boolean }[]>([]);
  let next = $state("");
  let error = $state("");
  let searched = $state(false);
  let asked = 0;
  let timer: ReturnType<typeof setTimeout>;
  let input: HTMLInputElement;

  function toRows(found: CatalogCard[], owned: number[]) {
    const d = addDefaults();
    return found.map((card, i) => ({
      card, owned: owned[i], qty: 1, busy: false,
      finish: card.finishes.includes(d.finish || "") ? d.finish! : card.finishes[0] || "nonfoil",
      condition: d.condition || "NM",
    }));
  }

  async function run(more = false) {
    const q = text.trim();
    if (!q) { cards = []; next = ""; searched = false; return; }
    const mine = ++asked;
    try {
      // "Shock M19" or "Opt DOM": a last word with a digit, or typed in capitals, is a set code.
      const words = q.split(/\s+/);
      const last = words.at(-1)!;
      const looksLikeSet = words.length > 1 && /^[a-z0-9]{3,5}$/i.test(last) && (/\d/.test(last) || last === last.toUpperCase());
      const code = setCode || (looksLikeSet ? words.pop()! : "");
      const p = new URLSearchParams({ text: words.join(" "), set: code });
      if (more && next) p.set("next", next);
      const r = await api<{ cards: CatalogCard[]; owned: number[]; next: string }>("GET", gameAPI(`/search?${p}`));
      if (mine !== asked) return; // a later search is on its way
      const rows = toRows(r.cards, r.owned);
      cards = more ? [...cards, ...rows] : rows;
      next = r.next;
      searched = true;
      error = "";
    } catch (e) { if (mine === asked) error = e instanceof Error ? e.message : String(e); }
  }

  export const focus = () => input?.focus();
  onMount(() => {
    loadSets().then((s) => (sets = s));
    if (found) { cards = toRows(found.cards, found.owned); searched = true; } else if (query.trim()) run();
    input.focus();
  });
</script>

<div class="add-search">
  <div class="search-row">
    <input bind:this={input} class="name" type="search" placeholder="Card name, or a name and set code" aria-label="Search for a card" autocomplete="off" bind:value={text}
      oninput={() => { clearTimeout(timer); if (text.trim().length >= 3) timer = setTimeout(() => run(), 400); }}
      onkeydown={(e) => { if (e.key === "Enter") { e.preventDefault(); clearTimeout(timer); run(); } }} />
    <input class="set" type="search" list="add-sets" placeholder="Any set" aria-label="Set" autocomplete="off" bind:value={setText}
      onchange={() => { if (text.trim()) run(); }} />
    <datalist id="add-sets">{#each sets as s (s.code)}<option value={s.code.toUpperCase()}>{s.name}</option>{/each}</datalist>
  </div>
  <div class="add-results">
    {#if error}<p class="error">{error}</p>{/if}
    {#each cards as r (r.card.id)}
      {@const price = r.card.market?.[r.finish] ?? Object.values(r.card.market || {})[0]}
      <div class="add-result">
        <Thumb card={r.card} />
        <div class="text">
          <span class="name">{r.card.name}</span>
          <span class="meta">{[sn(r.card), title(r.card.rarity), price ? usd(price) : "", r.owned ? `own ${r.owned}` : ""].filter(Boolean).join(" · ")}</span>
        </div>
        <div class="controls">
          <input type="number" min="1" step="1" bind:value={r.qty} aria-label="How many {r.card.name}" />
          <select aria-label="Finish" bind:value={r.finish}>{#each r.card.finishes.length ? r.card.finishes : ["nonfoil"] as f}<option value={f}>{fname(f)}</option>{/each}</select>
          <select aria-label="Condition" bind:value={r.condition}>{#each filters().conditions as c}<option value={c}>{c}</option>{/each}</select>
          <button type="button" class="primary" disabled={r.busy} onclick={async () => {
            r.busy = true;
            const quantity = Math.max(1, Math.round(r.qty) || 1);
            try {
              await onadd(r.card, { quantity, finish: r.finish, condition: r.condition });
              r.owned += quantity;
              r.qty = 1;
              // Ready for the next card: typing replaces the search.
              input.focus();
              input.select();
            } catch (e) { failed(e); }
            r.busy = false;
          }}>Add</button>
        </div>
      </div>
    {:else}
      <p class="muted empty">{searched ? "No cards found." : "Search for a card by name, or a name and set code (e.g. Shock M19)."}</p>
    {/each}
    {#if next}<button type="button" onclick={() => run(true)}>More results</button>{/if}
  </div>
</div>

<style>
  .search-row { display: flex; gap: 8px; }
  .search-row .name { flex: 1; min-width: 0; }
  .search-row .set { width: 11em; }
  .add-results { margin-top: 8px; }
  .add-result { display: grid; grid-template-columns: 34px minmax(0, 1fr) auto; gap: 10px; align-items: center; padding: 6px 2px; border-bottom: 1px solid var(--line); }
  .add-result .text { display: flex; flex-direction: column; min-width: 0; }
  .add-result .meta { color: var(--muted); font-size: 13px; }
  .add-result .controls { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; justify-content: flex-end; }
  .add-result .controls input { width: 4.2em; }
  .add-result .controls select { padding: 4px 6px; }
  @media (max-width: 800px) {
    .add-result { grid-template-columns: 34px minmax(0, 1fr); }
    .add-result .controls { grid-column: 1 / -1; justify-content: flex-start; }
  }
</style>
