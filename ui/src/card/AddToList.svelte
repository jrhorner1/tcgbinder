<!-- Putting a catalog card on a list (or a new one). -->
<script lang="ts">
  import { api, gameAPI, status, store } from "../lib/app.svelte";
  import { plural, sn } from "../lib/format";
  import type { CatalogCard, ListSummary } from "../lib/types";
  import Modal from "../components/Modal.svelte";

  let { card, quantity = 1, done }: { card: CatalogCard; quantity?: number; done: (v: boolean | null) => void } = $props();

  const NEW = "\u0000new";
  let lists = $state<ListSummary[]>([]);
  let which = $state(NEW);
  let name = $state("");
  let qty = $state(quantity);
  let anyPrinting = $state(true);
  let error = $state("");

  api<ListSummary[]>("GET", gameAPI("/lists")).then((l) => {
    lists = l;
    const last = store("lastList");
    which = l.find((x) => x.id === last)?.id || l[0]?.id || NEW;
  }).catch(() => {});

  async function save() {
    error = "";
    if (!Number.isInteger(qty) || qty < 1) { error = "Want at least 1 copy."; return; }
    try {
      let id = which;
      if (id === NEW) {
        if (!name.trim()) { error = "Name the new list."; return; }
        id = (await api("POST", gameAPI("/lists"), { name: name.trim() })).id;
      }
      const t = await api("POST", gameAPI(`/lists/entries?id=${encodeURIComponent(id)}`), { card, quantity: qty, any_printing: anyPrinting });
      store("lastList", id);
      status(`Added ${plural(qty, "copy", "copies")} of ${card.name} to ${t.name}.`);
      done(true);
    } catch (e) { error = e instanceof Error ? e.message : String(e); }
  }
</script>

<Modal title="Add to a list" subtitle="{card.name} · {sn(card)}" small {done}>
  <div class="form">
    <label for="l-which">List</label>
    <div class="stack">
      <select id="l-which" bind:value={which}>
        {#each lists as l}<option value={l.id}>{l.name}</option>{/each}
        <option value={NEW}>New list…</option>
      </select>
      {#if which === NEW}<input type="text" placeholder="Name, e.g. Mono-red burn" aria-label="New list's name" bind:value={name} />{/if}
    </div>
    <label for="l-qty">Copies wanted</label>
    <input id="l-qty" type="number" min="1" step="1" bind:value={qty} />
    <span></span>
    <label class="check"><input type="checkbox" bind:checked={anyPrinting} /> Any printing counts as owned</label>
  </div>
  <p class="note">Untick to count only this printing as owned.</p>
  <p class="error">{error}</p>
  {#snippet footer()}
    <button type="button" onclick={() => done(null)}>Cancel</button>
    <button type="button" class="primary" onclick={save}>Add to list</button>
  {/snippet}
</Modal>
