<!-- A modal dialog: opens when drawn, and answers null when closed (Escape, or the
     backdrop's Cancel). Give it a title, the body, and the footer's buttons. -->
<script lang="ts">
  import { onMount, type Snippet } from "svelte";

  let { title, subtitle = "", wide = false, small = false, done, children, footer }: {
    title: string;
    subtitle?: string;
    wide?: boolean;
    small?: boolean;
    /** Called with null when the dialog is closed without an answer. */
    done: (v: null) => void;
    children: Snippet;
    footer?: Snippet;
  } = $props();

  let dialog: HTMLDialogElement;
  onMount(() => {
    dialog.showModal();
    // Focus the first primary button, else the first field.
    const first = dialog.querySelector<HTMLElement>("[autofocus], input:not([type=hidden]), select, textarea, button.primary");
    first?.focus();
  });
</script>

<dialog bind:this={dialog} class:ask={small} class:wide onclose={() => done(null)}>
  <div class="dlg">
    <header>
      <h2>{title}</h2>
      {#if subtitle}<p class="muted sub">{subtitle}</p>{/if}
    </header>
    <div class="body">{@render children()}</div>
    {#if footer}<footer>{@render footer()}</footer>{/if}
  </div>
</dialog>

<style>
  .sub { margin: 4px 0 0; }
  dialog.wide { width: min(820px, 96vw); }
</style>
