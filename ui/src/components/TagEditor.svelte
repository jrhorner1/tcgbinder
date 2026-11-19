<!-- Editing a list of tags: chips with ×, then a box where Enter, a comma or a
     semicolon adds what's typed and Backspace takes off the last chip. Bind `tags`. -->
<script lang="ts">
  import { app } from "../lib/app.svelte";
  import { sameTag, sortTags, tagHue, tagProblem, TAGS_MAX, tidyTag } from "../lib/tags";

  let { tags = $bindable([]), label = "Tags" }: { tags: string[]; label?: string } = $props();

  let typed = $state("");
  let error = $state("");
  const id = `tags-${Math.random().toString(36).slice(2)}`;
  const known = $derived((app.filters?.tags || []).filter((k) => !tags.some((t) => sameTag(t, k))));

  /** Adds what's typed; false (with the reason shown) if part of it can't be a tag. */
  export function add(): boolean {
    error = "";
    const next = [...tags];
    for (const part of typed.split(/[,;]/).map(tidyTag).filter(Boolean)) {
      const problem = tagProblem(part);
      if (problem) { error = problem; return false; }
      if (next.some((t) => sameTag(t, part))) continue;
      if (next.length >= TAGS_MAX) { error = `At most ${TAGS_MAX} tags.`; return false; }
      next.push(app.filters?.tags.find((k) => sameTag(k, part)) || part); // the collection's spelling
    }
    typed = "";
    tags = sortTags(next);
    return true;
  }
  const remove = (t: string) => { tags = tags.filter((x) => !sameTag(x, t)); };
</script>

<div class="tag-editor">
  {#each tags as t (t)}
    <span class="tag-chip" style="--tag-hue:{tagHue(t)}">{t}<button type="button" class="tag-x" aria-label="Remove the tag {t}" title="Remove" onclick={() => remove(t)}>×</button></span>
  {/each}
  <input class="tag-input" type="text" list={id} placeholder={tags.length ? "" : "Add a tag"} aria-label={label} autocomplete="off" bind:value={typed}
    onkeydown={(e) => {
      if (e.key === "Enter" || e.key === "," || e.key === ";") { if (typed.trim() || e.key !== "Enter") { e.preventDefault(); add(); } }
      else if (e.key === "Backspace" && !typed && tags.length) remove(tags[tags.length - 1]);
    }}
    oninput={(e) => {
      // Picking a suggestion adds it straight away (it isn't typing).
      const picked = !(e instanceof InputEvent) || e.inputType === "insertReplacementText";
      if (picked && known.some((k) => sameTag(k, typed))) add();
    }}
    onblur={() => { if (typed.trim()) add(); }} />
  <datalist {id}>{#each known as k}<option value={k}></option>{/each}</datalist>
  {#if error}<span class="tag-error" role="alert">{error}</span>{/if}
</div>

<style>
  .tag-editor { display: flex; flex-wrap: wrap; gap: 4px; align-items: center; min-width: 0; padding: 3px 6px; border: 1px solid var(--line); border-radius: 7px; background: var(--panel); }
  .tag-editor:focus-within { border-color: var(--blue); }
  .tag-input { flex: 1 1 90px; min-width: 70px; border: 0; padding: 3px 2px; background: none; outline: none; }
  .tag-error { flex-basis: 100%; color: var(--red); font-size: 12px; }
</style>
