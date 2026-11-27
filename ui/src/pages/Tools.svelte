<!-- Tools: importing and exporting, tags, statistics, sealed product, the watchlist,
     backups, and refreshing prices. -->
<script lang="ts">
  import { api, app, exportCollection, gameAPI, runJob } from "../lib/app.svelte";
  import { askDialog } from "../lib/dialogs.svelte";
  import { plural } from "../lib/format";
  import ToolRow from "../components/ToolRow.svelte";

  interface HomeData { name: string; pricesAsOf: string; canLoadHistory: boolean; historyLoaded: string; historyInfo: string }
  const o = $derived.by(() => { void app.reloads; return api<HomeData>("GET", gameAPI("/home")); });
  const every = $derived({ daily: "automatically each day", weekly: "automatically each week", manual: "only when you ask" }[app.settings!.price_refresh]);
  const where = $derived(app.info!.client === "phone" ? "the app's folder in the Files app" : app.info!.saveExports ? "your Downloads folder" : "your downloads");

  async function exportAll() {
    const format = await askDialog<"csv" | "json">("Export the collection", [
      "A collection file keeps everything exactly, for a backup or to move to another device. CSV opens in spreadsheets and other collection apps.",
      "Import reads either back.",
    ], [{ label: "CSV", value: "csv" }, { label: "Collection file", value: "json", primary: true }]);
    if (format) exportCollection(format);
  }
</script>

<h1>Tools</h1>
{#await o then o}
  {#if app.info!.games.length > 1}<p class="muted">For {o.name}. Choose a game in the sidebar to switch.</p>{/if}
  <h2>Collection</h2>
  <div class="card-box">
    <ToolRow about={"Add cards from another app's CSV export (EchoMTG, ManaBox, Moxfield, Deckbox, TCGplayer and others) or from a collection file exported here."}>
      <a class="button" href="#/import">Import…</a>
    </ToolRow>
    <ToolRow about={`Save the collection to ${where}: as a collection file, for a backup or to move to another device, or as a CSV file for spreadsheets and other collection apps (a row per purchase, with its price paid, date, tags and market price). Import reads either back. To export only some cards, filter the collection and choose Export there.`}>
      <button type="button" onclick={exportAll}>Export…</button>
    </ToolRow>
    <ToolRow about={"Every tag on your cards, with how many cards and copies carry it and what they're worth. Rename or delete a tag everywhere at once."}>
      <a class="button" href="#/tags">Tags…</a>
    </ToolRow>
    <ToolRow about={"Your collection broken down by rarity, finish, condition, language, release year, color and card type, by price band, and by month acquired."}>
      <a class="button" href="#/stats">Statistics…</a>
    </ToolRow>
    <ToolRow about={"Boxes, bundles and decks you keep unopened: what you paid and what they're worth. There's no free price source for sealed product, so you enter each value yourself."}>
      <a class="button" href="#/sealed">Sealed product…</a>
    </ToolRow>
    <ToolRow about={"Cards whose price you're watching, with an alert when one moves by a percentage or crosses a price. Prices are checked when the app refreshes them, so alerts appear while the app is open or the next time you open it."}>
      <a class="button" href="#/watchlist">Watchlist…</a>
    </ToolRow>
    <ToolRow about={"Go back to the collection as it was before a recent import, bulk delete, restore or deleting all data. The last 10 backups are kept."}>
      <a class="button" href="#/backups">Restore a backup…</a>
    </ToolRow>
  </div>
  <h2>Prices</h2>
  <div class="card-box">
    <ToolRow about={`Fetch today's market prices now. ${o.pricesAsOf ? `Prices are from ${o.pricesAsOf}` : "No prices have been fetched yet"}; they update ${every} (see Settings).`}>
      <button type="button" onclick={() => runJob("/refresh", "Refreshing prices…", (r) => `Prices updated for ${plural(r.updated, "card", "cards")}`)}>Refresh prices now</button>
    </ToolRow>
    {#if o.canLoadHistory}
      <ToolRow about={o.historyLoaded ? `Price history last loaded ${o.historyLoaded}. Download ${o.historyInfo} again to fill in recent days.`
        : `Download ${o.historyInfo} so the value chart shows the past, not just the days the app was running.`}>
        <button type="button" onclick={() => runJob("/price-history", "Loading price history… this downloads about 50 MB.", (r) => `Loaded price history for ${plural(r.printings, "printing", "printings")}`)}>
          {o.historyLoaded ? "Update price history" : "Load price history"}</button>
      </ToolRow>
    {/if}
  </div>
{:catch e}<p class="error">{e.message}</p>{/await}
