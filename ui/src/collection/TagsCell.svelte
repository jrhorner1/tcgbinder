<!-- A table cell of tags: click one to filter by it, × to take it off, and type to add
     one (Enter adds it, Backspace in an empty box takes off the last). On a card's row
     a tag on only some of its copies is faded. -->
<script lang="ts">
  import { app, status } from "../lib/app.svelte";
  import { sameTag, tagHue, tagProblem, tidyTag } from "../lib/tags";

  let { tags, partial = [], onadd, onremove, onpick }: {
    tags: string[];
    /** Tags on only some of the copies (a card's row). */
    partial?: string[];
    onadd: (tag: string) => void;
    onremove: (tag: string) => void;
    onpick: (tag: string) => void;
  } = $props();

  let typed = $state("");

  function add() {
    const t = tidyTag(typed);
    if (!t) return;
    const problem = tagProblem(t);
    if (problem) { status(problem, true); return; }
    typed = "";
    if (tags.some((x) => sameTag(x, t)) && !partial.some((x) => sameTag(x, t))) return;
    onadd(app.filters?.tags.find((k) => sameTag(k, t)) || t); // the collection's spelling
  }
</script>

<div class="tagcell">
  {#each tags as t (t)}
    <span class="tag-chip" class:partial={partial.includes(t)} style="--tag-hue:{tagHue(t)}" title={partial.includes(t) ? "On some of the copies" : undefined}>
      <button type="button" class="tag-name" title="Show the cards with this tag" onclick={() => onpick(t)}>{t}</button>
      <button type="button" class="tag-x" aria-label="Remove the tag {t}" title="Remove" onclick={() => onremove(t)}>×</button>
    </span>
  {/each}
  <input class="cell tag-add" list="tag-names" aria-label="Add a tag" placeholder={tags.length ? "" : "+ tag"} bind:value={typed}
    onkeydown={(e) => {
      if (e.key === "Enter" || e.key === ",") { e.preventDefault(); add(); }
      else if (e.key === "Backspace" && !typed && tags.length) onremove(tags[tags.length - 1]);
      else if (e.key === "Escape") { typed = ""; e.currentTarget.blur(); }
    }}
    onchange={() => { if (app.filters?.tags.some((k) => sameTag(k, typed))) add(); }} />
</div>
