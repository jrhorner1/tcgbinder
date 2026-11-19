<!-- A table cell that's a menu: finish, condition, purchase. It reads "Mixed" on a
     card's row when its copies differ. -->
<script lang="ts">
  import { MIXED, type Shown } from "./cells";

  let { value, options, label, disabled = false, onchange }: {
    value: Shown<string>;
    options: [string, string][];
    label: string;
    disabled?: boolean;
    onchange: (value: string, text: string) => void;
  } = $props();
</script>

<select class="cell" class:mixed={value === MIXED} aria-label={label} {disabled} value={value === MIXED ? "\u0000" : value}
  onchange={(e) => onchange(e.currentTarget.value, e.currentTarget.selectedOptions[0].text)}>
  {#if value === MIXED}<option value={"\u0000"} disabled>Mixed</option>{/if}
  {#each options as [v, text]}<option value={v}>{text}</option>{/each}
</select>
