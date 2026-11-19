<!-- Every tag with what carries it, to rename or delete everywhere at once. -->
<script lang="ts">
  import { api, app, failed, gameAPI, loadFilters, reload, status } from "../lib/app.svelte";
  import { askName, confirmDialog } from "../lib/dialogs.svelte";
  import { plural, usd } from "../lib/format";
  import { sameTag, tagHue, tagProblem, tidyTag } from "../lib/tags";
  import type { TagTotal } from "../lib/types";

  const data = $derived.by(() => { void app.reloads; return api<{ tags: TagTotal[]; untagged: Omit<TagTotal, "name"> | null }>("GET", gameAPI("/tags")); });

  async function finish(msg: string, r: { undo: never }) { status(msg, false, r.undo); await loadFilters(); reload(); }

  async function rename(t: TagTotal, all: TagTotal[]) {
    const typed = await askName(`Rename “${t.name}”`, t.name, "New name");
    const name = typed && tidyTag(typed);
    if (!name || name === t.name) return;
    const problem = tagProblem(name);
    if (problem) { status(problem, true); return; }
    const other = all.find((x) => sameTag(x.name, name) && !sameTag(x.name, t.name));
    if (other && !(await confirmDialog(`“${other.name}” is already a tag. The two will be combined into one, “${other.name}”, on every card.`, { action: "Combine", title: "Combine the tags?" }))) return;
    try {
      const r = await api("PUT", gameAPI(`/tags?name=${encodeURIComponent(t.name)}`), { name });
      await finish(other ? `Combined “${t.name}” into “${other.name}”.` : `Renamed “${t.name}” to “${name}”.`, r);
    } catch (e) { failed(e); }
  }
  async function remove(t: TagTotal) {
    if (!(await confirmDialog(`Remove the tag “${t.name}” from ${plural(t.cards, "card", "cards")} (${plural(t.copies, "copy", "copies")})? The cards stay in your collection.`,
      { danger: true, action: "Delete tag", title: "Delete the tag?" }))) return;
    try { await finish(`Deleted the tag “${t.name}”.`, await api("DELETE", gameAPI(`/tags?name=${encodeURIComponent(t.name)}`))); } catch (e) { failed(e); }
  }
</script>

{#snippet figures(t: Omit<TagTotal, "name">)}
  <td class="n" data-label="Cards">{t.cards.toLocaleString()}</td>
  <td class="n" data-label="Copies">{t.copies.toLocaleString()}</td>
  <td class="n" data-label="Value">{usd(t.value)}</td>
  <td class="n" data-label="Paid">{usd(t.paid)}</td>
{/snippet}

<h1>Tags</h1>
<p class="muted">Tags are labels on your copies, such as where they're kept or which deck they're in. Add them in the collection table, when adding cards, or to a whole import. Totals count only the tagged copies.</p>
{#await data then d}
  {#if d.tags.length || d.untagged}
    <div class="card-box">
      <table class="tags-table">
        <thead><tr><th>Tag</th><th class="n">Cards</th><th class="n">Copies</th><th class="n">Market value</th><th class="n">Price paid</th><th><span class="sr">Actions</span></th></tr></thead>
        <tbody>
          {#each d.tags as t (t.name)}
            <tr>
              <td class="tag-name"><a class="tag-chip" href="#/collection?{new URLSearchParams({ tags: t.name })}" style="--tag-hue:{tagHue(t.name)}">{t.name}</a></td>
              {@render figures(t)}
              <td class="actions"><button type="button" onclick={() => rename(t, d.tags)}>Rename…</button> <button type="button" onclick={() => remove(t)}>Delete…</button></td>
            </tr>
          {/each}
          {#if d.untagged}
            <tr class="untagged"><td class="tag-name"><a href="#/collection?untagged=1">Untagged</a></td>{@render figures(d.untagged)}<td class="actions"></td></tr>
          {/if}
        </tbody>
      </table>
    </div>
    {#if !d.tags.length}<p class="muted">No tags yet. Type one in a card's Tags cell in the collection.</p>{/if}
  {:else}
    <p class="muted">No cards yet.</p>
  {/if}
{:catch e}<p class="error">{e.message}</p>{/await}

<style>
  .tags-table { width: 100%; border-collapse: collapse; font-variant-numeric: tabular-nums; }
  .tags-table th, .tags-table td { padding: 6px 8px; border-bottom: 1px solid var(--line); text-align: left; vertical-align: middle; }
  .tags-table th { font-size: 12px; color: var(--muted); font-weight: 600; }
  .tags-table .n { text-align: right; white-space: nowrap; }
  .tags-table .actions { text-align: right; white-space: nowrap; }
  .tags-table tr:last-child td { border-bottom: 0; }
  .tags-table .tag-name { max-width: 0; width: 34%; }
  /* On a phone each tag's row stacks, its figures labelled. */
  @media (max-width: 800px) {
    .tags-table thead { display: none; }
    .tags-table tr { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 2px 8px; padding: 8px 0; border-bottom: 1px solid var(--line); }
    .tags-table tr:last-child { border-bottom: 0; }
    .tags-table td { border: 0; padding: 0; text-align: left; }
    .tags-table .tag-name, .tags-table .actions { grid-column: 1 / -1; max-width: none; width: auto; }
    .tags-table .actions { text-align: left; padding-top: 4px; }
    .tags-table .actions:empty { display: none; }
    .tags-table td.n { text-align: left; font-size: 13px; }
    .tags-table td.n::before { content: attr(data-label); display: block; font-size: 11px; color: var(--muted); }
  }
</style>
