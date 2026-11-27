<!-- Editing a lot: its name, price, date, how many cards it holds, and how its price is
     split among them. Answers the saved lot. -->
<script lang="ts">
  import { api, gameAPI, status } from "../lib/app.svelte";
  import { plural } from "../lib/format";
  import type { Lot } from "../lib/types";
  import Modal from "../components/Modal.svelte";

  let { lot, done }: { lot: Lot; done: (saved: Lot | null) => void } = $props();

  const c = lot.count;
  let name = $state(lot.name);
  let total = $state(String(lot.total_paid));
  let date = $state(lot.acquired_date);
  let kind = $state<Lot["count"]["kind"]>(c.kind);
  let cards = $state(c.kind === "known" ? c.cards : 0);
  let packs = $state(c.kind === "bundle" ? c.packs : 9);
  let perPack = $state(c.kind === "bundle" ? c.cards_per_pack : 14);
  let extra = $state(c.kind === "bundle" ? c.extra_cards : 0);
  let lands = $state(c.kind === "bundle" ? c.lands : 0);
  let share = $state(c.kind === "bundle" ? c.lands_share_cost : false);
  let split = $state(lot.split);
  let note = $state(lot.note || "");
  let error = $state("");

  const count = $derived(kind === "known" ? { kind, cards: cards || 0 }
    : kind === "bundle" ? { kind, packs: packs || 0, cards_per_pack: perPack || 0, extra_cards: extra || 0, lands: lands || 0, lands_share_cost: share }
      : { kind });
  const preview = $derived.by(() => {
    const n = count;
    const divisor = n.kind === "known" ? n.cards : n.kind === "bundle" ? n.packs * n.cards_per_pack + n.extra_cards + (n.lands_share_cost ? n.lands : 0) : 0;
    if (split === "by_value") return "Each card's price is worked out from its market value when you finish the lot. Until then, cards carry an even share.";
    if (n.kind === "as_entered") return "The total is divided by the cards entered, so every card's price changes as you add more.";
    return divisor > 0 ? `${plural(divisor, "card", "cards")}, $${(Math.round(((parseFloat(total) || 0) / divisor) * 10000) / 10000).toFixed(4)} each` : "";
  });

  async function save() {
    error = "";
    try {
      const r = await api("PUT", gameAPI(`/lots?id=${encodeURIComponent(lot.id)}`),
        { name: name.trim(), total_paid: parseFloat(total) || 0, acquired_date: date, count, split, note: note.trim() });
      status(`Saved ${r.lot.name}.`);
      done(r.lot);
    } catch (e) { error = e instanceof Error ? e.message : String(e); }
  }
</script>

<Modal title="Edit lot" {done}>
  <div class="form">
    <label for="lot-name">Name</label><input id="lot-name" type="text" bind:value={name} />
    <label for="lot-total">Total paid</label><input id="lot-total" type="number" min="0" step="0.01" bind:value={total} />
    <label for="lot-date">Acquired</label><input id="lot-date" type="date" bind:value={date} />
    <label for="lot-kind">How many cards?</label>
    <select id="lot-kind" bind:value={kind}>
      <option value="known">I counted them</option>
      <option value="bundle">It's a bundle</option>
      <option value="as_entered">I'll count as I add</option>
    </select>
    {#if kind === "known"}
      <label for="lot-cards">Cards</label><input id="lot-cards" type="number" min="1" step="1" bind:value={cards} />
    {:else if kind === "bundle"}
      <label for="lot-packs">Packs</label><input id="lot-packs" type="number" min="0" step="1" bind:value={packs} />
      <label for="lot-per">Cards per pack</label><input id="lot-per" type="number" min="0" step="1" bind:value={perPack} />
      <label for="lot-extra">Other cards</label><input id="lot-extra" type="number" min="0" step="1" bind:value={extra} />
      <label for="lot-lands">Basic lands</label><input id="lot-lands" type="number" min="0" step="1" bind:value={lands} />
      <span></span><label class="check"><input type="checkbox" bind:checked={share} /> Lands share the cost</label>
    {/if}
    <label for="lot-split">Split the cost</label>
    <select id="lot-split" bind:value={split}>
      <option value="by_value">By each card's market value</option>
      <option value="even">Evenly</option>
    </select>
    <label for="lot-note">Note</label><input id="lot-note" type="text" bind:value={note} />
  </div>
  <p class="note">{preview}</p>
  <p class="error">{error}</p>
  {#snippet footer()}
    <button type="button" onclick={() => done(null)}>Cancel</button>
    <button type="button" class="primary" onclick={save}>Save</button>
  {/snippet}
</Modal>
