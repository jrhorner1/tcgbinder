<!-- The Add dialog: add a card, or a pack, bundle or lot (a booster box, a bundle, an
     eBay lot…) and then the cards in it, all in one place. Answers true when anything
     was added. -->
<script lang="ts">
  import { api, failed, gameAPI, loadLots, status, whileRunning } from "../lib/app.svelte";
  import { addCard, addDefaults, gameInfo } from "../lib/cards";
  import { fname, plural, sn, today, usd, usd4 } from "../lib/format";
  import { finishLot, lotProgress } from "../lib/lots";
  import { go } from "../lib/router.svelte";
  import type { CatalogCard, Lot } from "../lib/types";
  import ImportPreview, { type Preview } from "../components/ImportPreview.svelte";
  import TagEditor from "../components/TagEditor.svelte";
  import CardSearch, { type Picked } from "./CardSearch.svelte";

  let { lot = null, query = "", set = "", done }: { lot?: Lot | null; query?: string; set?: string; done: (changed: boolean | null) => void } = $props();

  // The standard contents the lot form's small buttons fill in: packs × cards per
  // pack, other cards (a promo, a toolkit's extra cards) and basic lands.
  const PRESETS = [
    { label: "Booster pack", packs: 1, per: 14, extra: 0, lands: 0 },
    { label: "Booster box", packs: 36, per: 14, extra: 0, lands: 0 },
    { label: "Collector booster box", packs: 12, per: 15, extra: 0, lands: 0 },
    { label: "Bundle", packs: 9, per: 14, extra: 1, lands: 40 },
    { label: "Prerelease kit", packs: 6, per: 14, extra: 1, lands: 0 },
    { label: "Deck builder's toolkit", packs: 4, per: 14, extra: 125, lands: 100 },
  ];

  let current = $state<Lot | null>(lot);
  let mode = $state<"card" | "lot" | "entering">(lot ? "entering" : "card");
  let changed = false;
  let added = $state<string[]>([]);
  const close = () => done(changed);

  // ---- Adding a card: the details that stay from one card to the next ----
  const d = addDefaults();
  let price = $state(d.price || "");
  let date = $state(d.date || "");
  let tags = $state<string[]>(d.tags || []);
  let tagEditor = $state<ReturnType<typeof TagEditor>>();
  const said = (c: CatalogCard, a: Picked) => `${a.quantity}× ${c.name} (${sn(c)}, ${fname(a.finish)}, ${a.condition})`;

  async function addOne(c: CatalogCard, a: Picked) {
    if (mode === "entering" && current) {
      const r = await addCard(c, { ...a, price: "", date: "", lot: current.id, tags: [] });
      if (r.lot) current = r.lot as Lot;
    } else {
      if (tagEditor && !tagEditor.add()) return;
      await addCard(c, { ...a, price, date, tags });
      status(`Added ${a.quantity}× ${c.name}.`);
    }
    changed = true;
    added = [said(c, a), ...added];
  }

  // ---- A pasted list: the import preview, into the lot when entering one ----
  let pasting = $state(false);
  let pasted = $state("");
  let preview = $state<Preview | null>(null);
  async function previewPaste() {
    if (!pasted.trim()) { status("Paste a list first."); return; }
    try { preview = await whileRunning(api<Preview>("POST", gameAPI("/import?preview=1"), new Blob([pasted], { type: "text/plain" }))); } catch (e) { failed(e); }
  }

  // ---- A new lot: blank, with buttons that fill in a product's standard contents ----
  let name = $state("");
  let total = $state("");
  let bought = $state(today());
  let packs = $state<number | null>(null), per = $state<number | null>(null), extra = $state<number | null>(null), lands = $state<number | null>(null);
  let split = $state<"by_value" | "even">("by_value");
  let preset = $state("");
  let lotError = $state("");
  const n = (x: number | null) => Math.max(0, Math.round(x || 0));
  const cardCount = $derived(n(packs) * (n(per) || 14) + n(extra));
  function fill(p: (typeof PRESETS)[number]) {
    packs = p.packs; per = p.per; extra = p.extra; lands = p.lands;
    if (!name.trim()) name = p.label;
    preset = p.label;
    lotError = "";
  }
  async function start() {
    lotError = "";
    if (!name.trim()) { lotError = "Give it a name."; return; }
    if (cardCount < 1) { lotError = "Say what's in it: packs or other cards."; return; }
    const count = n(packs) || n(lands)
      ? { kind: "bundle", packs: n(packs), cards_per_pack: n(per) || 14, extra_cards: n(extra), lands: n(lands), lands_share_cost: false }
      : { kind: "known", cards: n(extra) };
    try {
      current = await api<Lot>("POST", gameAPI("/lots"), { name: name.trim(), total_paid: parseFloat(total) || 0, acquired_date: bought, count, split });
      await loadLots();
      changed = true;
      mode = "entering";
    } catch (e) { lotError = e instanceof Error ? e.message : String(e); }
  }
  async function finish() {
    if (current && (await finishLot(current))) { changed = true; close(); }
  }
</script>

<dialog class="add-dialog" {@attach (el: HTMLDialogElement) => { el.showModal(); }} onclose={close}>
  <div class="dlg">
    <header>
      <div class="addhead">
        <h2>{mode === "card" ? "Add a card" : mode === "lot" ? "Add" : "Add cards to a lot"}</h2>
        {#if mode === "card"}
          <button type="button" onclick={() => { mode = current?.open ? "entering" : "lot"; }}>📦 A pack, bundle or lot…</button>
        {:else if mode === "lot"}
          <span class="toggle">
            <button type="button" class="on" aria-pressed="true">📦 A pack, bundle or lot</button>
            <button type="button" class="x" title="Back to adding a card" aria-label="Back to adding a card" onclick={() => (mode = "card")}>×</button>
          </span>
        {/if}
      </div>
    </header>
    <div class="body">
      {#if mode === "lot"}
        <div class="presets">
          <span class="muted small">Fill in:</span>
          {#each PRESETS as p}<button type="button" class:on={preset === p.label} onclick={() => fill(p)}>{p.label}</button>{/each}
        </div>
        <div class="form">
          <label for="lot-name">Name</label>
          <input id="lot-name" type="text" placeholder="e.g. Bloomburrow bundle, or a 500-card lot from eBay" bind:value={name} />
          <label for="lot-total">Total paid</label><input id="lot-total" type="number" min="0" step="0.01" placeholder="0.00" bind:value={total} />
          <label for="lot-date">Bought</label><input id="lot-date" type="date" bind:value={bought} />
          <span class="label">What's in it</span>
          <div class="contents">
            <input type="number" min="0" step="1" placeholder="0" aria-label="Packs" bind:value={packs} /><span>packs ×</span>
            <input type="number" min="0" step="1" placeholder="14" aria-label="Cards per pack" bind:value={per} /><span>+</span>
            <input type="number" min="0" step="1" placeholder="0" aria-label="Other cards" bind:value={extra} /><span>other cards +</span>
            <input type="number" min="0" step="1" placeholder="0" aria-label="Basic lands" bind:value={lands} /><span>basic lands</span>
            <span class="muted small">= {plural(cardCount, "card", "cards")}{n(lands) ? `, and ${n(lands)} basic lands that don't share the cost` : ""}</span>
          </div>
          <label for="lot-split">Split the cost</label>
          <select id="lot-split" bind:value={split}><option value="by_value">By each card's market value</option><option value="even">Evenly</option></select>
        </div>
        <p class="error">{lotError}</p>
      {:else}
        {#if mode === "entering" && current}
          {@const each = current.priceEach !== null ? `${usd4(current.priceEach)} each until it's finished` : `worth ${usd(current.marketValue)} so far`}
          <div class="lot-progress">
            <strong>📦 {current.name}</strong>
            <div class="muted small">{lotProgress(current)} · {each}</div>
            {#if current.expected}<div class="bar"><i style:width="{Math.min(100, (current.entered / current.expected) * 100)}%"></i></div>{/if}
          </div>
        {:else}
          <div class="form add-defaults">
            <label for="add-price">Price paid each</label><input id="add-price" type="number" min="0" step="0.01" placeholder="0.00" bind:value={price} />
            <label for="add-date">Acquired</label><input id="add-date" type="date" bind:value={date} />
            <span class="label">Tags</span><TagEditor bind:this={tagEditor} bind:tags label="Tags on the cards added" />
          </div>
        {/if}
        <CardSearch {query} {set} onadd={addOne} />
        <p class="add-links">
          <button type="button" class="link" onclick={() => (pasting = !pasting)}>Paste a list…</button>
          {#if mode === "card" && gameInfo().Scan}<button type="button" class="link" onclick={() => { close(); go("scan"); }}>Scan a card…</button>{/if}
        </p>
        {#if pasting}
          <div class="paste">
            <p class="muted small">One card per line: a count, the name, and optionally the set and collector number. Add *F* for foil.</p>
            <textarea rows="6" placeholder={"4 Lightning Bolt\n1 Sol Ring (C21) 263\n2 Shock [M19] *F*"} aria-label="Card list" bind:value={pasted}></textarea>
            <div class="buttons"><button type="button" onclick={previewPaste}>Preview</button></div>
            {#if preview}
              <ImportPreview {preview} fileName={mode === "entering" && current ? `pasted list for ${current.name}` : "pasted list"}
                lot={mode === "entering" ? current : null} cancel={() => (preview = null)}
                done={(r) => { changed = true; if (r.lot) current = r.lot; }} />
            {/if}
          </div>
        {/if}
        {#if added.length}
          <div class="added"><h3>Added just now</h3><ul>{#each added as a}<li>{a}</li>{/each}</ul></div>
        {/if}
      {/if}
    </div>
    <footer>
      {#if mode === "lot"}
        <button type="button" onclick={close}>Cancel</button>
        <button type="button" class="primary" onclick={start}>Start entering cards</button>
      {:else if mode === "entering" && current}
        <button type="button" onclick={() => { status(`Paused ${current!.name}: keep adding from its banner in the collection.`); close(); }}>Pause</button>
        <button type="button" class="primary" onclick={finish}>Finish</button>
      {:else}
        <button type="button" class="primary" onclick={close}>Done</button>
      {/if}
    </footer>
  </div>
</dialog>

<style>
  dialog.add-dialog { width: min(820px, 96vw); }
  .addhead { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .addhead h2 { margin: 0; flex: 1; }
  /* The lot button, chosen, with × to go back to adding a card. */
  .toggle { display: inline-flex; }
  .toggle .on { background: var(--blue-soft); border-color: var(--blue); border-top-right-radius: 0; border-bottom-right-radius: 0; }
  .toggle .x { background: var(--blue-soft); border-color: var(--blue); border-left: 0; border-top-left-radius: 0; border-bottom-left-radius: 0; padding: 6px 10px; }
  .presets { display: flex; flex-wrap: wrap; gap: 6px; align-items: center; margin-bottom: 8px; }
  .presets button { padding: 2px 10px; font-size: 13px; border-radius: 999px; }
  .presets button.on { background: var(--blue-soft); border-color: var(--blue); }
  .add-defaults { margin-top: 0; }
  .lot-progress { margin-bottom: 10px; }
  .add-links { display: flex; gap: 18px; flex-wrap: wrap; }
  .added h3 { font-size: 14px; color: var(--muted); margin: 14px 0 6px; }
  .added ul { margin: 0; padding-left: 18px; font-size: 14px; }
  .label { color: var(--muted); text-align: right; }
  /* What's in a lot reads as one line: "9 packs × 14 + 1 other cards + 40 basic lands". */
  .contents { display: flex; flex-wrap: wrap; gap: 6px; align-items: center; }
  .form .contents input { width: 4.5em; flex: none; }
  .contents .small { flex-basis: 100%; }
  .small { font-size: 13px; }
  @media (max-width: 800px) { .label { text-align: left; } }
</style>
