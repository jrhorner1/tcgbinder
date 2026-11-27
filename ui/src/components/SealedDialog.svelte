<!-- Adding a sealed item (found among a set's products, or named by hand), or editing
     one (`item`). -->
<script lang="ts" module>
  import { title } from "../lib/format";

  export interface SealedItem {
    id: string; product_uuid: string; name: string; set: string; category: string; subtype: string; quantity: number; price_paid: number;
    acquired_date: string; value: number; value_date: string; note: string; tcgplayer_product_id: number; purchase_url: string;
    valueStale: boolean; gain: number | null;
  }
  interface Product { uuid: string; name: string; set: string; category: string; subtype: string; tcgplayer_product_id: number; purchase_url: string }

  /** A product's category and subtype: "Booster box · Draft". */
  export const productKind = (p: { category: string; subtype: string }) =>
    [p.category, p.subtype !== "default" ? p.subtype : ""].filter(Boolean).map((x) => title(x.replaceAll("_", " "))).join(" · ");
</script>

<script lang="ts">
  import { api, gameAPI, settings, status } from "../lib/app.svelte";
  import { plural, today } from "../lib/format";
  import Modal from "./Modal.svelte";

  let { item, done }: { item: SealedItem | null; done: (v: boolean | null) => void } = $props();

  const it = item;
  let chosen = $state<Omit<Product, "name" | "set"> | null>(it?.product_uuid
    ? { uuid: it.product_uuid, category: it.category, subtype: it.subtype, tcgplayer_product_id: it.tcgplayer_product_id, purchase_url: it.purchase_url } : null);
  let set = $state(it?.set || "");
  let name = $state(it?.name || "");
  let quantity = $state(it?.quantity || 1);
  let paid = $state(it?.price_paid ? String(it.price_paid) : "");
  let date = $state(it?.acquired_date || (settings().default_date === "today" ? today() : ""));
  let value = $state(it?.value ? String(it.value) : "");
  let note = $state(it?.note || "");
  let products = $state<Product[]>([]);
  let picked = $state("");
  let found = $state("");
  let error = $state("");
  let finding = $state(false);

  async function find() {
    error = "";
    if (!set.trim()) { error = "Type the set's code first."; return; }
    finding = true;
    found = "Looking up the set's products…";
    try {
      products = (await api<{ products: Product[] }>("GET", gameAPI(`/sealed/products?set=${encodeURIComponent(set.trim())}`))).products;
      found = products.length ? `${plural(products.length, "product", "products")} in ${set.trim().toUpperCase()}.` : `No products listed for ${set.trim().toUpperCase()}; type the name.`;
    } catch (e) { found = ""; error = e instanceof Error ? e.message : String(e); }
    finding = false;
  }
  function choose() {
    const p = products[parseInt(picked, 10)];
    chosen = p || null;
    name = p ? p.name : "";
  }

  async function save() {
    error = "";
    const body = {
      name: name.trim(), set: set.trim(), quantity, price_paid: parseFloat(paid) || 0, acquired_date: date, value: parseFloat(value) || 0,
      value_date: it?.value_date || "", note: note.trim(), product_uuid: chosen?.uuid || "", category: chosen?.category || "", subtype: chosen?.subtype || "",
      tcgplayer_product_id: chosen?.tcgplayer_product_id || 0, purchase_url: chosen?.purchase_url || "",
    };
    if (!body.name) { error = "Choose a product or type its name."; return; }
    if (!(body.quantity >= 1)) { error = "The quantity must be at least 1."; return; }
    try {
      if (it) await api("PUT", gameAPI(`/sealed?id=${encodeURIComponent(it.id)}`), body);
      else await api("POST", gameAPI("/sealed"), body);
      status(it ? `Saved ${body.name}.` : `Added ${plural(body.quantity, "item", "items")}: ${body.name}.`);
      done(true);
    } catch (e) { error = e instanceof Error ? e.message : String(e); }
  }
</script>

<Modal title={it ? "Edit sealed product" : "Add sealed product"} small {done}>
  <div class="form">
    <label for="s-set">Set</label>
    <div class="inline"><input id="s-set" type="text" placeholder="Set code, e.g. M19" autocapitalize="characters" bind:value={set} /><button type="button" disabled={finding} onclick={find}>Find products</button></div>
    <label for="s-name">Product</label>
    <div class="stack">
      {#if products.length}
        <select aria-label="Product" bind:value={picked} onchange={choose}>
          <option value="">Choose a product, or type a name below</option>
          {#each products as p, i}<option value={String(i)}>{p.name} ({productKind(p)})</option>{/each}
        </select>
      {/if}
      <input id="s-name" type="text" placeholder="Product name" bind:value={name} />
    </div>
    <label for="s-qty">Quantity</label><input id="s-qty" type="number" min="1" step="1" bind:value={quantity} />
    <label for="s-paid">Price paid (each)</label><input id="s-paid" type="number" min="0" step="0.01" placeholder="Each" bind:value={paid} />
    <label for="s-date">Acquired</label><input id="s-date" type="date" bind:value={date} />
    <label for="s-value">Value (each)</label><input id="s-value" type="number" min="0" step="0.01" placeholder="Each, as you look it up" bind:value={value} />
    <label for="s-note">Note</label><input id="s-note" type="text" placeholder="Where it's kept, condition…" bind:value={note} />
  </div>
  {#if found}<p class="note">{found}</p>{/if}
  <p class="note">There's no free price source for sealed product, so the value is yours to enter: look it up (the product's link on the Sealed page helps) and update it now and then. Values older than 90 days are marked.</p>
  <p class="error">{error}</p>
  {#snippet footer()}
    <button type="button" onclick={() => done(null)}>Cancel</button>
    <button type="button" class="primary" onclick={save}>{it ? "Save" : "Add"}</button>
  {/snippet}
</Modal>

<style>
  .inline { display: flex; gap: 6px; }
  .inline input { flex: 1; min-width: 0; }
</style>
