<!-- A card's small picture in a list row, when Settings shows images. -->
<script lang="ts">
  import { app } from "../lib/app.svelte";

  let { card, size = 34 }: { card: { image_uri: string; name: string }; size?: number } = $props();
  // Scryfall's small image for its normal one; the normal one if that fails.
  const small = (uri: string) => uri.replace("/normal/", "/small/");
  let failed = $state(false);
</script>

{#if app.settings?.show_images && card.image_uri}
  <img class="thumb" style:width="{size}px" src={failed ? card.image_uri : small(card.image_uri)} alt="" loading="lazy" decoding="async"
    onerror={() => { failed = true; }} />
{/if}

<style>
  .thumb { aspect-ratio: 488 / 680; object-fit: cover; border-radius: 3px; flex: none; background: var(--panel-2); display: block; }
</style>
