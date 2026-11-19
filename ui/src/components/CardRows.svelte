<!-- A numbered list of cards you own (most valuable, most copies), each opening its
     page. -->
<script lang="ts">
  import { copiesSummary } from "../lib/cards";
  import { sn, usd } from "../lib/format";
  import { go } from "../lib/router.svelte";
  import type { Card } from "../lib/types";

  let { cards }: { cards: Card[] } = $props();
</script>

<div class="rows">
  {#each cards as c, i (c.key)}
    <button type="button" class="row" onclick={() => go("collection", { key: c.key })}>
      <span class="name">{i + 1}. {c.name}</span>
      <span class="where">{[sn(c), copiesSummary(c), c.price_from === "en" ? "English price" : ""].filter(Boolean).join(" · ")}</span>
      <span class="amt">{usd(c.market_value)}</span>
    </button>
  {/each}
</div>
