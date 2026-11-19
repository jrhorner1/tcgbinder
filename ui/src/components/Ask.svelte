<!-- A question with buttons (see askDialog in lib/dialogs.svelte.ts). -->
<script lang="ts">
  import type { Choice } from "../lib/dialogs.svelte";
  import Modal from "./Modal.svelte";

  let { title, message, buttons, cancel, done }: {
    title: string; message: string[]; buttons: Choice<unknown>[]; cancel: boolean; done: (v: unknown) => void;
  } = $props();
</script>

<Modal {title} small {done}>
  {#each message as m}<p>{m}</p>{/each}
  {#snippet footer()}
    {#if cancel}<button type="button" onclick={() => done(null)}>Cancel</button>{/if}
    {#each buttons as b}
      <button type="button" class:primary={b.primary} class:danger={b.danger} onclick={() => done(b.value)}>{b.label}</button>
    {/each}
  {/snippet}
</Modal>
