<!-- Asks for a name (see askName in lib/dialogs.svelte.ts). -->
<script lang="ts">
  import Modal from "./Modal.svelte";

  let { title, value, placeholder, done }: { title: string; value: string; placeholder: string; done: (v: string | null) => void } = $props();
  let name = $state(value);
  const save = () => { if (name.trim()) done(name.trim()); };
</script>

<Modal {title} small {done}>
  <input type="text" bind:value={name} aria-label="Name" {placeholder} onkeydown={(e) => e.key === "Enter" && save()} />
  {#snippet footer()}
    <button type="button" onclick={() => done(null)}>Cancel</button>
    <button type="button" class="primary" disabled={!name.trim()} onclick={save}>Save</button>
  {/snippet}
</Modal>
