// The watchlist: watched prices and their alerts (also shown on Home).

import { api, gameAPI } from "../api.js";
import { ALERT_NOTE, showWatch, thumbnail } from "../cards.js";
import { $, fill, h, status } from "../dom.js";
import { fname, signedUsd, sn, usd } from "../format.js";
import { route } from "../nav.js";

// ruleText describes a watch's rule: "a 20% move", "at or above $5.00".
function ruleText(r) {
  if (r.kind === "percent") return `a ${r.value}% move either way`;
  return `${r.kind === "above" ? "at or above" : "at or below"} ${usd(r.value)}`;
}

// The watch's change since its baseline: "+$0.05 (+25%)".
const watchChange = (w) => (w.change === null ? "" : `${signedUsd(w.change)} (${w.changePercent > 0 ? "+" : ""}${w.changePercent}%)`);

// watchAction dismisses or resets a watch's alert, or removes the watch.
async function watchAction(w, action) {
  try {
    if (action === "remove") await api("DELETE", gameAPI(`/watchlist?key=${encodeURIComponent(w.key)}`));
    else await api("PUT", gameAPI(`/watchlist?key=${encodeURIComponent(w.key)}`), { action });
    status({ dismiss: `Dismissed the alert for ${w.card.name}.`, reset: `${w.card.name} is now measured from today's price.`, remove: `Stopped watching ${w.card.name}.` }[action]);
    route();
  } catch (e) { status(e.message, true); }
}

// priceAlerts is Home's box of triggered watches.
export function priceAlerts(alerts) {
  if (!alerts.length) return null;
  return h("div.card-box.notice",
    h("h2", "Price alerts"),
    alerts.map((w) => h("div.alert-row",
      h("div.text", h("span.name", `${w.card.name} · ${fname(w.finish)}`),
        h("span.meta", `${usd(w.baseline)} on ${w.baseline_date} → ${usd(w.triggered.price)} on ${w.triggered.date}${w.change !== null ? ` · ${watchChange(w)}` : ""} · alert: ${ruleText(w.rule)}`)),
      h("div.row-actions",
        h("button", { type: "button", title: "Hide this alert; it comes back if the price returns and crosses again", onclick: () => watchAction(w, "dismiss") }, "Dismiss"),
        h("button", { type: "button", title: "Measure from today's price from now on", onclick: () => watchAction(w, "reset") }, "Reset")))),
    h("p.note", h("a", { href: "#/watchlist" }, "Watchlist"), ` · ${ALERT_NOTE}`));
}

// viewWatchlist lists the watched cards: price, baseline, change and alert.
export async function viewWatchlist() {
  const list = await api("GET", gameAPI("/watchlist"));
  const row = (w) => h("div.list-entry" + (w.triggered ? ".alerted" : ""),
    thumbnail(w.card, false),
    h("div.text",
      h("span.name", `${w.card.name}`),
      h("span.meta", [sn(w.card), fname(w.finish), `since ${w.baseline_date}: ${w.baseline ? usd(w.baseline) : "no price"} → ${w.price !== null ? usd(w.price) : "no price"}`].join(" · ")),
      h("span.meta", `Alert: ${ruleText(w.rule)}${w.triggered ? ` · triggered ${w.triggered.date} at ${usd(w.triggered.price)}` : ""}`)),
    h("div.entry-controls",
      h("span.change" + (w.change > 0 ? ".up" : w.change < 0 ? ".down" : ""), watchChange(w) || "—"),
      h("button", { type: "button", onclick: () => showWatch(w.card, w.finish, w) }, "Change…"),
      w.triggered ? h("button", { type: "button", onclick: () => watchAction(w, "dismiss") }, "Dismiss") : null,
      h("button", { type: "button", title: "Measure from today's price", onclick: () => watchAction(w, "reset") }, "Reset"),
      h("button", { type: "button", "aria-label": `Stop watching ${w.card.name}`, title: "Stop watching", onclick: () => watchAction(w, "remove") }, "✕")));
  fill($("#main"),
    h("h1", "Watchlist"),
    h("p.muted", ALERT_NOTE),
    list.length ? h("div.card-box", list.map(row))
      : h("p.muted", "Nothing watched yet. Choose Watch… on the Add page's card form, or Watch price… on a card you own."));
}
