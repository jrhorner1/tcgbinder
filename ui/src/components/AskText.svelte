<!-- Asks for some text in a box: a list's notes, a pasted card list. -->
<script lang="ts">
  import Modal from "./Modal.svelte";

  let { title, message = "", value = "", placeholder = "", rows = 6, action = "Save", done }: {
    title: string; message?: string; value?: string; placeholder?: string; rows?: number; action?: string; done: (v: string | null) => void;
  } = $props();
  let text = $state(value);
</script>

<Modal {title} {done}>
  {#if message}<p>{message}</p>{/if}
  <textarea {rows} {placeholder} aria-label={title} bind:value={text}></textarea>
  {#snippet footer()}
    <button type="button" onclick={() => done(null)}>Cancel</button>
    <button type="button" class="primary" onclick={() => done(text)}>{action}</button>
  {/snippet}
</Modal>
