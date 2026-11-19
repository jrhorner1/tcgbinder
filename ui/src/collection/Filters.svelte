<!-- The collection's filters: the search, the main menus, More filters, and a chip for
     each filter in use that removes it. They live in the address (#/collection?set=…),
     so Back and bookmarks keep them. -->
<script lang="ts">
  import { app, filters } from "../lib/app.svelte";
  import { choiceNames, fname, languageName, plural, title, usd } from "../lib/format";
  import { sameTag, tagHue } from "../lib/tags";
  import type { TagTotal } from "../lib/types";

  let { q, nav, tagTotals }: {
    q: URLSearchParams;
    /** Changes the filters: `mutate` edits a copy of the address's parameters. */
    nav: (mutate: (p: URLSearchParams) => void, replace?: boolean) => void;
    tagTotals: TagTotal[];
  } = $props();

  const LIST_KEYS = ["key", "sort", "dir"];
  const MORE_KEYS = ["condition", "language", "card_type", "price_min", "price_max", "acquired_from", "acquired_to", "min_copies", "no_price_paid", "unpriced", "tags", "tags_not", "untagged"];
  const FLAGS: [string, string][] = [["no_price_paid", "No price paid"], ["unpriced", "No market price"], ["untagged", "Untagged"]];

  const f = $derived(filters());
  // The game's first filter (Magic's Color) stays in view; the rest go in More filters.
  const firstFilter = $derived(f.filters[0]);
  const otherFilters = $derived(f.filters.slice(1));
  const paramTags = (p: URLSearchParams, name: string) => (p.get(name) || "").split(",").map((t) => t.trim()).filter(Boolean);
  const chosen = $derived(paramTags(q, "tags"));
  const excluded = $derived(paramTags(q, "tags_not"));
  const active = $derived(MORE_KEYS.filter((k) => q.get(k)).length + otherFilters.filter((x) => q.get("f." + x.Name)).length);
  let moreOpen = $state(false);

  const set = (name: string, value: string) => nav((p) => { if (value) p.set(name, value); else p.delete(name); });

  let typed = $state(q.get("q") || "");
  let timer: ReturnType<typeof setTimeout>;
  const search = () => { clearTimeout(timer); timer = setTimeout(() => nav((p) => { if (typed) p.set("q", typed); else p.delete("q"); }, true), 250); };

  // A tag in More filters cycles: chosen → excluded (struck through) → off.
  function cycle(t: string) {
    nav((p) => {
      const inc = paramTags(p, "tags"), exc = paramTags(p, "tags_not");
      const put = (name: string, list: string[]) => (list.length ? p.set(name, list.join(",")) : p.delete(name));
      if (inc.some((x) => sameTag(x, t))) { put("tags", inc.filter((x) => !sameTag(x, t))); put("tags_not", [...exc, t]); }
      else if (exc.some((x) => sameTag(x, t))) put("tags_not", exc.filter((x) => !sameTag(x, t)));
      else put("tags", [...inc, t]);
      if (paramTags(p, "tags").length < 2) p.delete("tags_mode");
    });
  }

  function chipLabel(k: string, v: string) {
    const flag = FLAGS.find(([name]) => name === k);
    if (flag) return flag[1];
    switch (k) {
      case "q": return `“${v}”`;
      case "set": return `Set: ${v.toUpperCase()}`;
      case "rarity": return `Rarity: ${title(v)}`;
      case "finish": return `Finish: ${fname(v)}`;
      case "condition": return `Condition: ${choiceNames.default_condition[v] || v}`;
      case "language": return `Language: ${languageName(v)}`;
      case "card_type": return `Type: ${v}`;
      case "lot": return v === "none" ? "Bought in no lot" : `Bought in: ${app.lots.find((l) => l.id === v)?.name || v}`;
      case "price_min": return `Price ≥ ${usd(Number(v))}`;
      case "price_max": return `Price ≤ ${usd(Number(v))}`;
      case "acquired_from": return `Acquired from ${v}`;
      case "acquired_to": return `Acquired to ${v}`;
      case "min_copies": return `${v}+ copies`;
      default: return k.startsWith("f.") ? `${k.slice(2)}: ${v}` : `${k}: ${v}`;
    }
  }
  const chips = $derived([
    ...[...q.entries()].filter(([k, v]) => v && k !== "q" && !LIST_KEYS.includes(k) && !["tags", "tags_not", "tags_mode"].includes(k))
      .map(([k, v]) => ({ label: chipLabel(k, v), remove: (p: URLSearchParams) => p.delete(k) })),
    ...([["tags", "Tag"], ["tags_not", "Not"]] as const).flatMap(([k, word]) => paramTags(q, k).map((t) => ({
      label: `${word}: ${t}`,
      remove: (p: URLSearchParams) => {
        const left = paramTags(p, k).filter((x) => !sameTag(x, t));
        if (left.length) p.set(k, left.join(",")); else p.delete(k);
        if (paramTags(p, "tags").length < 2) p.delete("tags_mode");
      },
    }))),
  ]);
</script>

{#snippet menu(name: string, label: string, options: { value: string; label: string }[])}
  <!-- The address may spell a value in another case (set=m19 for M19). -->
  {@const chosen = options.find((o) => o.value.toLowerCase() === (q.get(name) || "").toLowerCase())?.value || ""}
  <select class:on={!!q.get(name)} aria-label={label} value={chosen} onchange={(e) => set(name, e.currentTarget.value)}>
    <option value="">{label}</option>
    {#each options as o}<option value={o.value}>{o.label}</option>{/each}
  </select>
{/snippet}

<div class="filters">
  <input type="search" placeholder="Filter by name, set, number or tag" aria-label="Filter the collection" bind:value={typed} oninput={search} />
  <div class="menus">
    <span class="set-menu">{@render menu("set", "Set", f.sets.map((s) => ({ value: s.Code, label: s.Label })))}</span>
    {#if firstFilter}{@render menu("f." + firstFilter.Name, firstFilter.Name, firstFilter.Options.map((o) => ({ value: o, label: o })))}{/if}
    {@render menu("rarity", "Rarity", f.rarities.map((r) => ({ value: r, label: title(r) })))}
    {@render menu("finish", "Finish", f.finishes.map((x) => ({ value: x, label: fname(x) })))}
    {#if app.lots.length}
      {@render menu("lot", "Bought in", [{ value: "none", label: "No lot" }, ...app.lots.map((l) => ({ value: l.id, label: `📦 ${l.name}` }))])}
    {/if}
    <button type="button" class="more-toggle" class:on={active > 0} aria-expanded={moreOpen} onclick={() => (moreOpen = !moreOpen)}>
      More filters{active ? ` (${active})` : ""} {moreOpen ? "▴" : "▾"}
    </button>
  </div>

  {#if moreOpen}
    <div class="more">
      <div class="menus">
        {#each otherFilters as x}{@render menu("f." + x.Name, x.Name, x.Options.map((o) => ({ value: o, label: `${x.Name}: ${o}` })))}{/each}
        {@render menu("condition", "Condition", f.conditions.map((c) => ({ value: c, label: choiceNames.default_condition[c] || c })))}
        {#if f.languages.length}{@render menu("language", "Language", f.languages.map((l) => ({ value: l.code, label: l.name })))}{/if}
      </div>
      <div class="more-fields">
        {#each [["card_type", "Type", "text", "e.g. creature"], ["min_copies", "At least", "number", "copies"], ["price_min", "Price from", "number", "$"],
          ["price_max", "Price to", "number", "$"], ["acquired_from", "Acquired from", "date", ""], ["acquired_to", "Acquired to", "date", ""]] as [name, label, type, hint]}
          <label>{label}<input class:on={!!q.get(name)} {type} placeholder={hint} value={q.get(name) || ""} step={name.startsWith("price") ? "0.01" : undefined}
            onchange={(e) => set(name, e.currentTarget.value.trim())} /></label>
        {/each}
      </div>
      {#if tagTotals.length}
        <div class="tag-filter">
          <div class="tag-filter-head">
            <span>Tags</span>
            {#if chosen.length >= 2}
              <div class="ranges" role="group" aria-label="Cards with">
                {#each [["all", "All of these"], ["any", "Any of these"]] as [v, label]}
                  <button type="button" class:on={(q.get("tags_mode") === "any" ? "any" : "all") === v}
                    onclick={() => nav((p) => (v === "any" ? p.set("tags_mode", "any") : p.delete("tags_mode")))}>{label}</button>
                {/each}
              </div>
            {/if}
          </div>
          <div class="tag-chips">
            {#each tagTotals as t (t.name)}
              {@const s = chosen.some((x) => sameTag(x, t.name)) ? "on" : excluded.some((x) => sameTag(x, t.name)) ? "not" : "off"}
              <button type="button" class="tag-chip {s}" style="--tag-hue:{tagHue(t.name)}" aria-pressed={s !== "off"} onclick={() => cycle(t.name)}
                title={{ on: "Chosen; click to exclude it", not: "Excluded; click to clear", off: "Click to choose" }[s]}>
                {t.name}<span class="count"> {t.cards}</span>
              </button>
            {/each}
          </div>
          <p class="note">Click a tag to show cards with it, again to show cards without it, a third time to clear it.</p>
        </div>
      {/if}
      <div class="flags">
        {#each FLAGS as [name, label]}
          <label class="check"><input type="checkbox" checked={q.get(name) === "1"} onchange={(e) => set(name, e.currentTarget.checked ? "1" : "")} />{label}</label>
        {/each}
      </div>
      <p class="note">Price is one copy's market price. Acquired matches any purchase in the range.</p>
    </div>
  {/if}

  {#if chips.length}
    <div class="chips">
      {#each chips as c}
        <button type="button" class="chip" aria-label="Remove the filter {c.label}" onclick={() => nav(c.remove)}>{c.label}<span aria-hidden="true"> ×</span></button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .filters { display: grid; gap: 8px; }
  .filters input[type=search] { width: 100%; }
  .menus { display: flex; flex-wrap: wrap; gap: 6px; }
  .menus select { flex: 1 1 108px; min-width: 0; }
  .menus select[aria-label="Bought in"] { flex-basis: 140px; }
  select.on, input.on { border-color: var(--blue); background: var(--blue-soft); }
  /* Set names are long: the set menu is wider than the others, but stays in the row. */
  .set-menu select { flex: 2 1 180px; max-width: 280px; text-overflow: ellipsis; }
  .more-fields { display: grid; grid-template-columns: repeat(auto-fill, minmax(118px, 1fr)); gap: 6px 10px; }
  .more-fields label { display: grid; gap: 2px; font-size: 13px; color: var(--muted); }
  .more-fields input { width: 100%; min-width: 0; }
  /* More filters' tags: chosen, excluded (struck through) or off. */
  .tag-filter-head { display: flex; flex-wrap: wrap; gap: 6px 12px; align-items: center; justify-content: space-between; font-size: 13px; color: var(--muted); margin-bottom: 6px; }
  .tag-chips { display: flex; flex-wrap: wrap; gap: 5px; }
  .tag-filter .tag-chip { font-size: 13px; padding: 2px 10px; }
  .tag-filter .tag-chip.off { background: none; color: var(--text); box-shadow: inset 0 0 0 1px var(--line); }
  .tag-filter .tag-chip.on { box-shadow: inset 0 0 0 2px hsl(var(--tag-hue) 60% var(--tag-fg)); }
  .tag-filter .tag-chip.not { text-decoration: line-through; opacity: 0.75; }
  .tag-filter .count { opacity: 0.7; }
  .tag-filter .note { margin: 6px 0 0; }
  /* A chip for each filter in use, which removes it. */
  .chips { display: flex; flex-wrap: wrap; gap: 6px; }
  .chip { font-size: 13px; padding: 3px 10px; border-radius: 999px; border-color: var(--blue); background: var(--blue-soft); }
  .more { border: 1px solid var(--line); border-radius: var(--radius); padding: 10px; background: var(--panel); display: grid; gap: 8px; }
  .more .menus { display: flex; flex-wrap: wrap; gap: 6px; }
  .more .flags { display: flex; flex-wrap: wrap; gap: 4px 16px; font-size: 14px; }
  .more .flags label { display: flex; gap: 6px; align-items: center; }
  .more .note { margin: 0; }
  .more-toggle { color: var(--blue); }
  .more-toggle.on { border-color: var(--blue); background: var(--blue-soft); }
  .set-menu { display: contents; }
</style>
