<!-- Settings: appearance, the collection, prices, adding cards, the data folder,
     deleting all data. Changes are saved right away. -->
<script lang="ts">
  import { api, app, loadFilters, loadLots, reload, saveSettings } from "../lib/app.svelte";
  import { changeDataFolder, chooseDataFolder, dataFolder } from "../lib/datafolder";
  import { confirmDialog, openDialog } from "../lib/dialogs.svelte";
  import { choiceNames } from "../lib/format";
  import type { Settings } from "../lib/types";
  import DeleteAll from "../components/DeleteAll.svelte";
  import ToolRow from "../components/ToolRow.svelte";

  interface SettingsData { settings: Settings; choices: Record<string, string[]>; defaults: Settings; dataDir: string }
  const data = $derived.by(() => { void app.reloads; return Promise.all([api<SettingsData>("GET", "/api/settings"), dataFolder()]); });
  const phone = app.info!.client === "phone";
  let folderPath = $state("");

  async function restoreDefaults(defaults: Settings) {
    if (!(await confirmDialog("Put every setting back to how it was when the app was installed? Your collection isn't affected.", { action: "Restore defaults" }))) return;
    await saveSettings(defaults, "defaults restored");
    reload();
  }
  async function deleteAll() {
    if (!(await openDialog(DeleteAll))) return;
    await loadFilters();
    await loadLots();
    location.hash = "#/home";
  }
</script>

{#snippet menu(d: SettingsData, key: string, what: string)}
  <select id="set-{key}" value={String(d.settings[key])} onchange={(e) => saveSettings({ [key]: e.currentTarget.value }, what)}>
    {#each d.choices[key] as v}<option value={v}>{choiceNames[key]?.[v] || v}</option>{/each}
  </select>
{/snippet}
{#snippet check(d: SettingsData, key: string, label: string, what: string)}
  <label class="check"><input type="checkbox" id="set-{key}" checked={!!d.settings[key]} onchange={(e) => saveSettings({ [key]: e.currentTarget.checked }, what)} />{label}</label>
{/snippet}
{#snippet hint(text: string)}<div class="hint">{text}</div>{/snippet}

<h1>Settings</h1>
<p class="muted">Changes are saved right away and apply to every game.</p>
{#await data then [d, df]}
  <h2>Appearance</h2>
  <div class="card-box"><div class="form settings" style="margin:0">
    <label for="set-theme">Theme</label><div>{@render menu(d, "theme", "theme")}{@render hint("Light, dark, or follow the system setting")}</div>
    <label for="set-text_size">Text size</label><div>{@render menu(d, "text_size", "text size")}{@render hint("Scales text and spacing")}</div>
    <label for="set-show_images">Card images</label><div>{@render check(d, "show_images", "Show card images", "card images")}{@render hint("Turn off to save data on slow or metered connections")}</div>
  </div></div>
  <h2>Collection</h2>
  <div class="card-box"><div class="form settings" style="margin:0">
    <label for="set-card_order">Card order</label><div>{@render menu(d, "card_order", "card order")}{@render hint("How cards are listed in the collection, until you sort it")}</div>
    <label for="set-set_completion">Set completion</label><div>{@render menu(d, "set_completion", "set completion")}{@render hint("What a set's completion counts: the cards numbered up to the size printed on them, or every card including variants, promos and extras")}</div>
    <label for="set-chart_range">Value chart</label><div>{@render menu(d, "chart_range", "chart range")}{@render hint("The range the Value Over Time chart shows first")}</div>
  </div></div>
  <h2>Prices</h2>
  <div class="card-box"><div class="form settings" style="margin:0">
    <label for="set-price_refresh">Update prices</label><div>{@render menu(d, "price_refresh", "price updates")}{@render hint("You can also refresh them on the Tools page")}</div>
    <label for="set-auto_price_history">Price history</label><div>{@render check(d, "auto_price_history", "Load price history automatically", "price history")}{@render hint("The first time, on desktop and web (about a 50 MB download); phones ask first")}</div>
  </div></div>
  <h2>Adding Cards</h2>
  <div class="card-box"><div class="form settings" style="margin:0">
    <label for="set-default_condition">Condition</label><div>{@render menu(d, "default_condition", "default condition")}{@render hint("Preselected when adding a card")}</div>
    <label for="set-default_date">Acquired date</label><div>{@render menu(d, "default_date", "default date")}{@render hint("Filled in when adding a card")}</div>
  </div></div>
  <h2>Advanced</h2>
  <div class="card-box"><div class="form settings" style="margin:0">
    <label for="set-confirm_delete">Deleting</label><div>{@render check(d, "confirm_delete", "Ask before deleting cards", "delete confirmation")}</div>
    <label for="set-log_level">Log detail</label><div>{@render menu(d, "log_level", "log level")}{@render hint(phone ? "How much goes in the log file" : "How much goes in the log file; --log-level overrides it at startup")}</div>
  </div></div>

  <h2>Data</h2>
  <p class="muted">Your collections, price history and backups are kept in this folder. Choose a folder in iCloud Drive, Dropbox or OneDrive to share them between devices. Settings stay on each device.</p>
  {#if !df}
    <p class="settings-path"><code>{d.dataDir || "(not saved)"}</code></p>
  {:else}
    <p class="settings-path"><code>{df.path}</code>{#if df.isDefault}<span class="muted"> (the default)</span>{/if}</p>
    {#if df.fixed}
      <p class="muted">Set by --data-dir or TCGBINDER_DATA_DIR when TCG Binder was started.</p>
    {:else if !phone}
      {#if df.canPick}
        <div class="buttons"><div class="right">
          <button type="button" onclick={() => chooseDataFolder(df)}>Change…</button>
          {#if !df.isDefault}<button type="button" onclick={() => changeDataFolder(df, "")}>Use the default folder</button>{/if}
        </div></div>
      {:else}
        <div class="search">
          <input type="text" placeholder="/full/path/to/a/folder" aria-label="Data folder path" bind:value={folderPath} />
          <button type="button" onclick={() => folderPath.trim() && changeDataFolder(df, folderPath.trim())}>Use this folder</button>
        </div>
        <p class="muted">A folder on the computer running TCG Binder.</p>
        {#if !df.isDefault}<p><button type="button" onclick={() => changeDataFolder(df, "")}>Use the default folder</button></p>{/if}
      {/if}
    {/if}
  {/if}
  <div class="card-box danger-zone">
    <ToolRow about="Delete your whole collection and everything with it (lots, sales, lists, watchlist, sealed product, price history), for every game. You'll be asked to confirm.">
      <button type="button" class="danger" onclick={deleteAll}>Delete all data…</button>
    </ToolRow>
  </div>

  <h2>About</h2>
  <p>{app.info!.name} {app.info!.version}</p>
  <p><button type="button" onclick={() => restoreDefaults(d.defaults)}>Restore defaults</button></p>
{:catch e}<p class="error">{e.message}</p>{/await}

<style>
  .form.settings { grid-template-columns: 160px minmax(0, 520px); align-items: start; margin: 0; }
  .settings label { padding-top: 7px; }
  .settings select { min-width: 220px; }
  .settings label.check { padding-top: 0; }
  .hint { color: var(--muted); font-size: 13px; margin-top: 3px; }
  .settings-path code { overflow-wrap: anywhere; }
  .danger-zone { margin-top: 12px; }
  @media (max-width: 800px) {
    .form.settings { grid-template-columns: 1fr; }
    .settings label { text-align: left; padding-top: 0; }
    .settings select { min-width: 0; width: 100%; }
  }
</style>
