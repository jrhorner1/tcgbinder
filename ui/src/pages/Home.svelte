<!-- Home: the collection's value and its chart, set completion and value by set, the
     most valuable cards, price alerts, and notices about the data folder. -->
<script lang="ts">
  import { api, app, gameAPI, runJob, saveSettings } from "../lib/app.svelte";
  import { dataFolder } from "../lib/datafolder";
  import { fname, gain, plural, signedUsd, usd } from "../lib/format";
  import { go } from "../lib/router.svelte";
  import type { Card } from "../lib/types";
  import { ALERT_NOTE, ruleText, watchAction, watchChange, type Watch } from "../lib/watchlist";
  import Bars from "../components/Bars.svelte";
  import CardRows from "../components/CardRows.svelte";
  import DataNotices from "../components/DataNotices.svelte";
  import LostLots from "../components/LostLots.svelte";
  import LineChart, { type Series } from "../components/LineChart.svelte";
  import Tile from "../components/Tile.svelte";

  interface SetRow { Code: string; Name: string; Size: number; Collected: number; Completion: number; MarketValue: number }
  interface HomeData {
    name: string; cards: number; uniqueCards: number; sets: number; pricePaid: number; pricePaidUnknown: number; marketValue: number; unpriced: number; pricesAsOf: string;
    chart: { marketValue: Series["points"]; pricePaid: Series["points"]; estimate: Series["points"]; from: string; to: string; historyStart: string };
    bySet: SetRow[]; topCards: Card[]; canLoadHistory: boolean; historyLoaded: string; historyInfo: string; realisedThisYear: number; hasSales: boolean;
    alerts: Watch[]; sealed: { value: number; items: number; unvalued: number } | null;
  }

  let range = $state(app.settings!.chart_range);
  const load = $derived.by(() => { void app.reloads; return Promise.all([api<HomeData>("GET", gameAPI(`/home?range=${encodeURIComponent(range)}`)), dataFolder()]); });

  async function setRange(r: string) { range = r; await saveSettings({ chart_range: r }, "chart range"); }
  const toSet = (code: string) => () => go("collection", { set: code });
</script>

{#await load}
  <p class="muted">Loading…</p>
{:then [o, df]}
  {@const g = gain(o.pricePaid, o.marketValue)}
  {@const series = [
    { name: "Market value", color: "var(--blue)", points: o.chart.marketValue },
    { name: "Price paid", color: "var(--amber)", points: o.chart.pricePaid, step: true },
    { name: "At today's prices (estimate)", color: "var(--grey)", points: o.chart.estimate, step: true },
  ]}
  {@const completion = o.bySet.map((x) => ({ onclick: toSet(x.Code), label: `${x.Code} · ${x.Name}`, value: x.Size > 0 ? x.Completion : 0,
    text: x.Size > 0 ? `${x.Collected} / ${x.Size}  (${Math.round(x.Completion * 100)}%)` : `${x.Collected} owned` }))}
  {@const byValue = o.bySet.filter((x) => x.MarketValue > 0).sort((a, b) => b.MarketValue - a.MarketValue).slice(0, 10)
    .map((x) => ({ onclick: toSet(x.Code), label: `${x.Code} · ${x.Name}`, value: x.MarketValue, text: usd(x.MarketValue) }))}
  <h1>{o.name}</h1>
  {#if df}<DataNotices {df} cards={o.cards} />{/if}
  <LostLots />
  {#if o.alerts?.length}
    <div class="card-box notice">
      <h2>Price alerts</h2>
      {#each o.alerts as w (w.key)}
        <div class="alert-row">
          <div class="text">
            <span class="name">{w.card.name} · {fname(w.finish)}</span>
            <span class="meta">{usd(w.baseline)} on {w.baseline_date} → {usd(w.triggered!.price)} on {w.triggered!.date}{w.change !== null ? ` · ${watchChange(w)}` : ""} · alert: {ruleText(w.rule)}</span>
          </div>
          <div class="row-actions">
            <button type="button" title="Hide this alert; it comes back if the price returns and crosses again" onclick={() => watchAction(w, "dismiss")}>Dismiss</button>
            <button type="button" title="Measure from today's price from now on" onclick={() => watchAction(w, "reset")}>Reset</button>
          </div>
        </div>
      {/each}
      <p class="note"><a href="#/watchlist">Watchlist</a> · {ALERT_NOTE}</p>
    </div>
  {/if}
  <div class="tiles">
    <Tile cap="CARDS" value={o.cards.toLocaleString()} detail="{plural(o.uniqueCards, 'unique card', 'unique cards')} · {plural(o.sets, 'set', 'sets')}" />
    <Tile cap="PRICE PAID" value={usd(o.pricePaid)} detail={o.pricePaidUnknown ? `${plural(o.pricePaidUnknown, "card", "cards")} without a price paid` : "every card has a price paid"} />
    <Tile cap="MARKET VALUE" value={usd(o.marketValue)} color="var(--blue)" detail="prices as of {o.pricesAsOf || 'never'}{o.unpriced ? ` · ${o.unpriced} unpriced` : ''}">
      {#if o.sealed}<br /><a href="#/sealed">plus {usd(o.sealed.value)} sealed</a>{/if}
    </Tile>
    <Tile cap="GAIN / LOSS" value={g.text} color={g.color} detail={[g.pct, o.hasSales ? `realised: ${signedUsd(o.realisedThisYear)} this year` : ""].filter(Boolean).join(" · ")} />
  </div>
  <div class="section-head">
    <h2>Value Over Time</h2>
    <div class="ranges">
      {#each app.info!.ranges as r}<button type="button" class:on={r === range} onclick={() => setRange(r)}>{r}</button>{/each}
    </div>
  </div>
  <div class="card-box">
    <LineChart {series} from={o.chart.from} to={o.chart.to} />
    <p class="note">
      {#if o.canLoadHistory && !o.historyLoaded}Market value uses daily prices recorded each day the app runs. Load {o.historyInfo} to fill in the history.
      {:else if o.chart.historyStart}Market value: daily market prices since {o.chart.historyStart}.{/if}
      Price paid: running total by acquired date.
      {#if o.chart.estimate.length}Before recorded prices begin, the grey line values the cards you owned at today's prices.{/if}
    </p>
    {#if o.canLoadHistory && !o.historyLoaded}
      <button type="button" onclick={() => runJob("/price-history", "Loading price history… this downloads about 50 MB.", (r) => `Loaded price history for ${plural(r.printings, "printing", "printings")}`)}>Load price history</button>
    {/if}
  </div>
  <div class="two-col">
    <div><h2>Set Completion</h2>{#if completion.length}<div class="card-box"><Bars items={completion} max={1} color="var(--blue)" /></div>{:else}<p class="muted">No sets yet.</p>{/if}</div>
    <div><h2>Value by Set</h2>{#if byValue.length}<div class="card-box"><Bars items={byValue} max={byValue[0].value} color="var(--amber)" /></div>{:else}<p class="muted">No market prices yet.</p>{/if}</div>
  </div>
  <h2>Most Valuable Cards</h2>
  {#if o.topCards.length}<div class="card-box"><CardRows cards={o.topCards} /></div>{:else}<p class="muted">No market prices yet; they refresh automatically each day.</p>{/if}
{:catch e}
  <p class="error">{e.message}</p>
{/await}

<style>
  /* A triggered price watch. */
  .alert-row { display: flex; flex-wrap: wrap; gap: 6px 12px; align-items: center; padding: 6px 0; border-bottom: 1px solid var(--line); }
  .alert-row .text { flex: 1 1 220px; min-width: 0; display: flex; flex-direction: column; }
  .alert-row .meta { color: var(--muted); font-size: 13px; }
</style>
