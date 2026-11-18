// The Home page: the collection's value and chart, sets, top cards, alerts and the
// buttons for refreshing prices and loading price history.

import { api, dataFolder, gameAPI, runJob, saveSettings } from "../api.js";
import { cardRows } from "../cards.js";
import { bars, gain, legend, lineChart, tile } from "../charts.js";
import { $, fill, h } from "../dom.js";
import { plural, signedUsd, usd } from "../format.js";
import { go, route } from "../nav.js";
import { state } from "../state.js";
import { dataNotices } from "./settings.js";
import { priceAlerts } from "./watchlist.js";

export async function viewHome() {
  const range = state.range;
  const [o, df] = await Promise.all([api("GET", gameAPI(`/home?range=${encodeURIComponent(range)}`)), dataFolder()]);
  const g = gain(o.pricePaid, o.marketValue);
  const series = [
    { name: "Market value", color: "var(--blue)", points: o.chart.marketValue },
    { name: "Price paid", color: "var(--amber)", points: o.chart.pricePaid, step: true },
    { name: "At today's prices (estimate)", color: "var(--grey)", points: o.chart.estimate, step: true },
  ];
  const notes = [];
  if (o.canLoadHistory && !o.historyLoaded) notes.push(`Market value uses daily prices recorded each day the app runs. Load ${o.historyInfo} to fill in the history.`);
  else if (o.chart.historyStart) notes.push(`Market value: daily market prices since ${o.chart.historyStart}.`);
  notes.push("Price paid: running total by acquired date.");
  if (o.chart.estimate.length) notes.push("Before recorded prices begin, the grey line values the cards you owned at today's prices.");

  const loadHistory = o.canLoadHistory && !o.historyLoaded
    ? h("button", { type: "button", onclick: (e) => runJob(e.currentTarget, "/price-history", "Loading price history… this downloads about 50 MB.", (r) => `Loaded price history for ${plural(r.printings, "printing", "printings")}`) }, "Load price history")
    : null;

  const completion = o.bySet.map((x) => ({
    onclick: () => go("collection", new URLSearchParams({ set: x.Code })),
    label: `${x.Code} · ${x.Name}`,
    value: x.Size > 0 ? x.Completion : 0,
    text: x.Size > 0 ? `${x.Collected} / ${x.Size}  (${Math.round(x.Completion * 100)}%)` : `${x.Collected} owned`,
  }));
  const byValue = o.bySet.filter((x) => x.MarketValue > 0).sort((a, b) => b.MarketValue - a.MarketValue).slice(0, 10)
    .map((x) => ({ onclick: () => go("collection", new URLSearchParams({ set: x.Code })), label: `${x.Code} · ${x.Name}`, value: x.MarketValue, text: usd(x.MarketValue) }));

  fill($("#main"),
    h("h1", o.name),
    dataNotices(df, o),
    priceAlerts(o.alerts || []),
    h("div.tiles",
      tile("CARDS", o.cards.toLocaleString(), `${plural(o.uniqueCards, "unique card", "unique cards")} · ${plural(o.sets, "set", "sets")}`),
      tile("PRICE PAID", usd(o.pricePaid), o.pricePaidUnknown ? `${plural(o.pricePaidUnknown, "card", "cards")} without a price paid` : "every card has a price paid"),
      tile("MARKET VALUE", usd(o.marketValue), [`prices as of ${o.pricesAsOf || "never"}${o.unpriced ? ` · ${o.unpriced} unpriced` : ""}`,
        ...(o.sealed ? [h("br"), h("a", { href: "#/sealed" }, `plus ${usd(o.sealed.value)} sealed`)] : [])], "var(--blue)"),
      tile("GAIN / LOSS", g.text, [g.pct, o.hasSales ? `realised: ${signedUsd(o.realisedThisYear)} this year` : ""].filter(Boolean).join(" · "), g.color)),
    h("div.section-head", h("h2", "Value Over Time"),
      h("div.ranges", state.info.ranges.map((r) => h("button" + (r === range ? ".on" : ""), {
        type: "button", onclick: async () => { state.range = r; await saveSettings({ chart_range: r }, "chart range"); route(); } }, r)))),
    h("div.card-box", legend(series), lineChart(series, o.chart.from, o.chart.to, $("#main").clientWidth - 70), h("p.note", notes.join(" ")), loadHistory),
    h("div.two-col",
      h("div", h("h2", "Set Completion"), completion.length ? h("div.card-box", bars(completion, 1, "var(--blue)")) : h("p.muted", "No sets yet.")),
      h("div", h("h2", "Value by Set"), byValue.length ? h("div.card-box", bars(byValue, byValue[0].value, "var(--amber)")) : h("p.muted", "No market prices yet."))),
    h("h2", "Most Valuable Cards"),
    o.topCards.length ? h("div.card-box", cardRows(o.topCards, (c) => c.market_value)) : h("p.muted", "No market prices yet; they refresh automatically each day."),
  );
}
