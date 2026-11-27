<!-- A figure at the top of a page: a caption, the number, and a line about it. -->
<script lang="ts">
  import type { Snippet } from "svelte";

  let { cap, value, detail = "", color = "", children }: { cap: string; value: string; detail?: string; color?: string; children?: Snippet } = $props();
</script>

<div class="tile">
  <div class="cap">{cap}</div>
  <div class="val" style:color={color || null}>{value}</div>
  <div class="det">{detail}{@render children?.()}</div>
</div>

<style>
  .tile { background: var(--panel); border: 1px solid var(--line); border-radius: var(--radius); padding: 12px 16px; }
  .cap { font-size: 12px; color: var(--muted); letter-spacing: .04em; font-weight: 600; }
  .val { font-size: 25px; font-weight: 700; margin: 3px 0; font-variant-numeric: tabular-nums; }
  .det { font-size: 12px; color: var(--muted); }
</style>
