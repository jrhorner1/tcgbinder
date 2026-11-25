<!-- One card: its picture, the card's details, watching its price and adding it to a
     list. A card you own also shows its value and your copies (the same editable rows
     as the collection table), and changing its printing. A card you don't own (opened
     from the catalog: its key is its catalog ID) offers Add to collection instead. -->
<script lang="ts">
  import { SvelteSet } from "svelte/reactivity";
  import { api, ApiError, app, failed, filters, gameAPI, loadFilters, status } from "../lib/app.svelte";
  import { ask, openDialog } from "../lib/dialogs.svelte";
  import { fname, gain, languageName, plural, sn, title, usd } from "../lib/format";
  import { go, params } from "../lib/router.svelte";
  import { copiesSummary } from "../lib/cards";
  import type { Card } from "../lib/types";
  import { openAdd } from "../add/add";
  import Tile from "../components/Tile.svelte";
  import Table from "../collection/Table.svelte";
  import AddToList from "./AddToList.svelte";
  import Printings from "./Printings.svelte";
  import WatchDialog from "./WatchDialog.svelte";

  let { key }: { key: string } = $props();

  interface CardInfo { card: Card; raw: unknown; details: { Label: string; Value: string }[]; link: { url: string; label: string }; others: Card[] }
  let info = $state<CardInfo | null>(null);
  let error = $state("");
  let cards = $state<Card[]>([]);
  let showPrintings = $state(false);
  let showRaw = $state(false);
  // Whether you own this printing; if not, the page shows the catalog's card.
  let owned = $state(true);

  $effect(() => {
    void app.reloads;
    const k = key;
    (async () => {
      try {
        const d = await api<CardInfo>("GET", gameAPI(`/card?key=${encodeURIComponent(k)}`));
        info = d; cards = [d.card]; owned = true; error = "";
      } catch (e) {
        if (!(e instanceof ApiError && e.status === 404)) { error = e instanceof Error ? e.message : String(e); return; }
        try {
          const d = await api<Omit<CardInfo, "raw">>("GET", gameAPI(`/catalog?id=${encodeURIComponent(k)}`));
          info = { ...d, raw: null }; cards = []; owned = false; error = "";
        } catch (e2) { error = e2 instanceof Error ? e2.message : String(e2); }
      }
    })();
  });
  // The table's edits replace the card: keep the figures in step (or leave, if it's gone).
  const c = $derived(cards[0] || info?.card);
  $effect(() => { if (owned && info && !cards.length) close(); });

  function close() { const p = params(); p.delete("key"); go("collection", p); }
  const open = (k: string) => { const p = params(); p.set("key", k); go("collection", p); };

  async function remove() {
    if (!c || !(await ask(`Remove all ${c.copy_count} of ${c.name} (${sn(c)}) from your collection?`, "Remove"))) return;
    try {
      await api("DELETE", gameAPI(`/card?key=${encodeURIComponent(key)}`));
      status(`Deleted ${c.name}`);
      await loadFilters();
      close();
    } catch (e) { failed(e); }
  }

  const rank = (f: string) => { const i = filters().finishes.indexOf(f); return i < 0 ? 99 : i; };
</script>

{#if error}
  <p class="error">{error}</p>
{:else if info && c}
  {@const unpaid = c.purchases.filter((p) => !p.price_paid).reduce((n, p) => n + p.quantity, 0)}
  {@const unpriced = c.purchases.filter((p) => !p.market).reduce((n, p) => n + p.quantity, 0)}
  {@const g = c.market_value && c.price_paid ? gain(c.price_paid, c.market_value) : null}
  {@const market = Object.entries(c.market || {}).sort((a, b) => rank(a[0]) - rank(b[0])).map(([k, v]) => `${usd(v)} ${fname(k)}`).join(" · ")}
  <div class="detail card-page">
    <button type="button" class="link back-link" onclick={close}>← Collection</button>
    <div class="detail-body">
      <div class="image">
        {#if app.settings?.show_images && c.image_uri}
          <img class="cardimg" src={c.image_uri} alt={c.name} loading="lazy" />
        {:else if !app.settings?.show_images}
          <div class="cardimg off">Card images are off (Settings)</div>
        {/if}
        {#if info.link.url}<p><a href={info.link.url} target="_blank" rel="noopener noreferrer">{info.link.label}</a></p>{/if}
      </div>
      <div class="main">
        {#if owned}<div class="tiles">
          <Tile cap="MARKET VALUE" value={c.market_value ? usd(c.market_value) : "—"} color="var(--blue)"
            detail={c.market_value ? [plural(c.copy_count, "copy", "copies"), unpriced ? `${unpriced} unpriced` : "", c.price_from === "en" ? "English price" : ""].filter(Boolean).join(" · ") : "no market price yet"} />
          <Tile cap="PRICE PAID" value={c.price_paid ? usd(c.price_paid) : "—"}
            detail={c.price_paid ? (unpaid ? `${unpaid} without a price paid` : c.copy_count > 1 ? `${usd(c.price_paid / c.copy_count)} average` : "") : "price paid not recorded"} />
          <Tile cap="GAIN / LOSS" value={g ? g.text : "—"} detail={g ? g.pct : "needs both prices"} color={g?.color || ""} />
        </div>{/if}
        <h1>{c.name}</h1>
        <p class="muted">{[c.set_name, sn(c), title(c.rarity)].filter(Boolean).join(" · ")}</p>
        {#if c.type_line}<p><strong>{c.type_line}</strong></p>{/if}
        {#if c.oracle_text}<p class="rules">{c.oracle_text}</p>{/if}
        <dl class="kv">
          <dt>Set</dt><dd>{c.set_name} ({c.set})</dd>
          <dt>Collector number</dt><dd>{c.collector_number}</dd>
          <dt>Rarity</dt><dd>{title(c.rarity)}</dd>
          {#each info.details || [] as d}<dt>{d.Label}</dt><dd>{d.Value}</dd>{/each}
          <dt>Language</dt><dd>{languageName(c.lang || "en")}</dd>
          {#if c.released_at}<dt>Released</dt><dd>{c.released_at}</dd>{/if}
          <dt>Printed in</dt><dd>{c.finishes.map(fname).join(", ")}</dd>
          <dt>Market prices</dt>
          <dd>{market || "not available yet"}{c.price_from === "en" ? " — the English printing's price: the catalog has none for this language" : ""}</dd>
        </dl>
        <p class="side-buttons">
          <button type="button" onclick={() => openDialog(WatchDialog, { card: c })}>Watch price…</button>
          <button type="button" onclick={() => openDialog(AddToList, { card: c, quantity: 1 })}>Add to list…</button>
          {#if owned}<button type="button" aria-expanded={showRaw} onclick={() => (showRaw = !showRaw)}>Raw data</button>{/if}
        </p>
      </div>
      <div class="copies">
        {#if !owned}
          <div class="section-head">
            <h2>Your Copies</h2>
            <button type="button" class="primary" onclick={() => openAdd({ card: c })}>Add to collection…</button>
          </div>
          <p class="muted">You don't own this printing yet.</p>
        {:else}
        {#if showRaw}<pre class="raw">{JSON.stringify(info.raw, null, 2)}</pre>{/if}
        <div class="section-head">
          <h2>Your Copies</h2>
          <button type="button" onclick={() => openAdd({ query: c.name, set: c.set })}>+ Add copies…</button>
        </div>
        <Table bind:cards shows={null} sort="" dir="asc" selection={new SvelteSet()} onsort={() => {}} onfilter={(name, v) => go("collection", { [name]: v })}
          onopen={() => {}} openAll compact />
        <div class="buttons">
          <p class="note">Each row is a group of copies alike in everything but how many. Change a cell and it's saved; Split moves a copy to a group of its own.</p>
          <div class="right">
            <button type="button" aria-expanded={showPrintings} onclick={() => (showPrintings = !showPrintings)}>Change printing…</button>
            <button type="button" class="danger" onclick={remove}>Delete</button>
          </div>
        </div>
        {#if showPrintings}<Printings card={c} onmoved={(k) => { showPrintings = false; open(k); }} />{/if}
        {/if}

        {#if info.others?.length}
          <h2>Other Printings You Own</h2>
          <div class="rows">
            {#each info.others as o (o.key)}
              <button type="button" class="row" onclick={() => open(o.key)}><span class="name">{o.set_name} · {sn(o)}</span><span class="where">{copiesSummary(o)}</span></button>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  </div>
{:else}
  <p class="muted">Loading…</p>
{/if}

<style>
  .main { min-width: 0; }
  .main h1 {
    padding-top: 16px;
  }
  .detail-body { 
    display: grid;
    gap: 20px;
    grid-template-columns: minmax(0, 380px) minmax(0, 1fr);
  }
  .cardimg { 
    width: 100%;
    max-width: 350px;
    margin-inline: auto;
    aspect-ratio: 488 / 680; 
    border-radius: 2cqw;
    overflow: hidden;
    background: var(--panel-2); 
    object-fit: contain; 
    display: block; 
  }
  .cardimg.off { 
    display: flex; 
    align-items: center; 
    justify-content: center; 
    text-align: center; 
    color: var(--muted); 
    font-size: 13px; 
    padding: 12px; 
    aspect-ratio: auto; 
    height: 120px; 
  }
  .image p { text-align: center; }
  .side-buttons { 
    display: flex; 
    gap: 6px; 
    justify-content: center; 
    flex-wrap: wrap; 
  }
  .kv { display: grid; grid-template-columns: auto 1fr; gap: 4px 14px; }
  .kv dt { color: var(--muted); text-align: right; }
  .kv dd { margin: 0; }
  .rules { white-space: pre-wrap; }
  @media (max-width: 800px) { .cardimg { margin: 0 auto; width: min(250px, 70vw); } }
  .card-page { max-width: 1600px; }
  .back-link { margin-bottom: 8px; }
  .tiles { grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); }
  @media (max-width: 800px) { .detail-body { grid-template-columns: minmax(0, 1fr); } }
  .copies {
    grid-column: 1 / -1;
  }
</style>
