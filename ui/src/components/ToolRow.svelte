<!-- One tool: its button or link, and what it does. -->
<script lang="ts">
  import type { Snippet } from "svelte";

  let { about, children }: { about: string; children: Snippet } = $props();
</script>

<div class="tool">{@render children()}<p class="muted">{about}</p></div>

<style>
  .tool { display: grid; grid-template-columns: 220px 1fr; gap: 14px; align-items: baseline; padding: 10px 0; border-bottom: 1px solid var(--line); }
  .tool:last-child { border-bottom: 0; }
  .tool > :global(.button), .tool > :global(button) { justify-content: flex-start; }
  .tool p { margin: 0; }
  @media (max-width: 800px) { .tool { grid-template-columns: 1fr; gap: 4px; } }
</style>
