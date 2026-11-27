<!-- Home's notices about the data folder: the first-run welcome, and copies of the
     data another device saved at the same time (to merge or discard). -->
<script lang="ts">
  import { api, app, failed, gameAPI, loadFilters, reload, status, store } from "../lib/app.svelte";
  import { chooseDataFolder, type DataFolder } from "../lib/datafolder";
  import { confirmDialog } from "../lib/dialogs.svelte";
  import { plural } from "../lib/format";

  let { df, cards }: { df: DataFolder; cards: number } = $props();

  let welcomed = $state(!!store("welcomed"));
  const ABOUT: Record<string, [string, string]> = {
    lots: ["your lots", "Merging adds the lots only it has; where both have a lot, the newer one is kept."],
    sales: ["your sales", "Merging adds the sales only it has; none is counted twice."],
    lists: ["your lists", "Merging adds the lists and cards only it has; where both have a card on a list, the larger quantity is kept."],
  };
  const about = (kind: string) => ABOUT[kind] || ["your collection", "Merging adds its cards and copies to yours without counting any twice; a card deleted on one device comes back."];

  async function act(c: DataFolder["conflicts"][number], what: "merge" | "discard") {
    if (what === "discard" && !(await confirmDialog(`Delete ${c.name}? Its changes are lost.`, { danger: true, action: "Discard it" }))) return;
    try {
      const r = await api("POST", gameAPI(`/conflicts/${what}`), { name: c.name });
      const unit: [string, string] = ({ lots: ["lot", "lots"], sales: ["sale", "sales"], lists: ["list", "lists"] } as Record<string, [string, string]>)[c.kind] || ["card", "cards"];
      status(what === "merge" ? `Merged ${c.name}: ${plural(r.changed, ...unit)} changed.` : `Discarded ${c.name}.`, false, r.undo);
      await loadFilters();
      reload();
    } catch (e) { failed(e); }
  }
</script>

{#if df.firstRun && !cards && !welcomed}
  <div class="card-box notice">
    <h2>Welcome to TCG Binder</h2>
    <p>Your collection will be kept in {df.path}. Keep it there, or choose a folder (for example in iCloud Drive or Dropbox to share it between devices).</p>
    <div class="buttons"><div class="right">
      <button type="button" onclick={() => { store("welcomed", "1"); welcomed = true; }}>Keep it there</button>
      {#if df.canPick}
        <button type="button" class="primary" onclick={() => chooseDataFolder(df)}>Choose a folder…</button>
      {:else}
        <a class="button" href="#/settings">Choose a folder…</a>
      {/if}
    </div></div>
  </div>
{/if}
{#each df.conflicts.filter((c) => c.game === app.game) as c (c.name)}
  <div class="card-box notice">
    <h2>Another device saved a different version of {about(c.kind)[0]}</h2>
    <p>{c.name}{c.kind === "collection" && c.cards !== null ? ` (${plural(c.cards, "unique card", "unique cards")})` : ""} was saved by another device at the same time as this one. {about(c.kind)[1]} Discarding deletes that file.</p>
    <div class="buttons"><div class="right">
      <button type="button" onclick={() => act(c, "discard")}>Discard it</button>
      <button type="button" class="primary" onclick={() => act(c, "merge")}>Merge it</button>
    </div></div>
  </div>
{/each}
