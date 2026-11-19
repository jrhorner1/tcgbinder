<!-- Labelled bars: one row per item, its bar's length its share of `max`. An item
     with `onclick` has a label you can click. -->
<script lang="ts" module>
  export interface Bar { label: string; value: number; text: string; onclick?: (() => void) | null }
</script>

<script lang="ts">
  let { items, max, color }: { items: Bar[]; max: number; color: string } = $props();
</script>

<div class="bars">
  {#each items as it}
    {#if it.onclick}
      <button type="button" class="label link" title={it.label} onclick={it.onclick}>{it.label}</button>
    {:else}
      <div class="label" title={it.label}>{it.label}</div>
    {/if}
    <div class="track"><div class="fill" style:width="{Math.max(0, Math.min(1, it.value / (max || 1))) * 100}%" style:background={color}></div></div>
    <div class="num">{it.text}</div>
  {/each}
</div>

<style>
  .bars { display: grid; grid-template-columns: minmax(110px, 38%) 1fr auto; gap: 7px 10px; align-items: center; font-size: 14px; }
  .label { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  button.label { display: block; width: 100%; padding: 0; border: 0; border-radius: 0; background: none; font: inherit; color: var(--blue); text-align: left; cursor: pointer; }
  .track { height: 9px; background: var(--panel-2); border-radius: 5px; overflow: hidden; }
  .fill { height: 100%; border-radius: 5px; }
  .num { color: var(--muted); text-align: right; white-space: nowrap; font-variant-numeric: tabular-nums; }
  @media (max-width: 800px) { .bars { grid-template-columns: minmax(90px, 40%) 1fr auto; } }
</style>
