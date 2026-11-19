<!-- A table cell you type in: price paid, acquired date, quantity. It saves when you
     leave it or press Enter, and Escape puts back what was there. -->
<script lang="ts">
  import { MIXED, type Shown } from "./cells";

  let { value, kind, label, disabled = false, title = "", onchange }: {
    /** The value shown (a number for money and quantity, a YYYY-MM-DD date). */
    value: Shown<string | number>;
    kind: "money" | "date" | "qty";
    label: string;
    disabled?: boolean;
    title?: string;
    onchange: (value: number | string) => void;
  } = $props();

  const shown = $derived(value === MIXED ? "" : kind === "money" ? (value ? Number(value).toFixed(2) : "") : String(value ?? ""));
  let input = $state() as HTMLInputElement;

  function commit() {
    const raw = input.value.trim();
    if (raw === shown) return;
    if (kind === "date") { onchange(raw); return; }
    const n = kind === "qty" ? Math.round(Number(raw)) : parseFloat(raw.replace(/[$,]/g, "") || "0");
    if (!Number.isFinite(n) || n < (kind === "qty" ? 1 : 0)) { input.value = shown; return; }
    onchange(n);
  }
</script>

{#if disabled}
  <span class="cell readonly" class:r={kind !== "date"} {title}>{kind === "money" ? (value === MIXED ? "Mixed" : value ? `$${Number(value).toFixed(2)}` : "—") : value === MIXED ? "Mixed" : value || "—"}</span>
{:else}
  <input bind:this={input} class="cell" class:r={kind !== "date"} class:mixed={value === MIXED} aria-label={label} {title}
    type={kind === "date" ? "date" : kind === "qty" ? "number" : "text"} inputmode={kind === "money" ? "decimal" : undefined} min={kind === "qty" ? 1 : undefined}
    placeholder={value === MIXED ? "Mixed" : kind === "money" ? "—" : ""} value={shown}
    onchange={commit}
    onkeydown={(e) => {
      if (e.key === "Enter") input.blur();
      if (e.key === "Escape") { input.value = shown; input.blur(); }
    }} />
{/if}
