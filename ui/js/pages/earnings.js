// Earnings: the cards sold, by month and year, and undoing a sale.

import { api, gameAPI, loadFilters } from "../api.js";
import { tile } from "../charts.js";
import { $, askDialog, fill, h, status } from "../dom.js";
import { fname, plural, signedUsd, sn, usd } from "../format.js";
import { go, route } from "../nav.js";

// viewEarnings lists sold cards with totals, for one year or all of them.
export async function viewEarnings(qs) {
  const q = new URLSearchParams(qs);
  const year = q.get("year") || "";
  const d = await api("GET", gameAPI(`/sales?year=${encodeURIComponent(year)}`));
  const t = d.totals;
  const years = h("select", { "aria-label": "Year", onchange: (e) => go("earnings", new URLSearchParams(e.target.value ? { year: e.target.value } : {})) },
    h("option", { value: "" }, "All years"),
    [...new Set([...d.years, year].filter(Boolean))].map((y) => h("option", { value: y, selected: y === year }, y)));
  const undo = async (s) => {
    const what = `${plural(s.quantity, "copy", "copies")} of ${s.card.name}`;
    const choice = await askDialog("Undo this sale?", [
      `${what}, sold ${s.date} for ${usd(s.proceeds)}.`,
      "Putting the copies back returns them to your collection as the purchase they were, and deletes the sale. Deleting the record only leaves them sold.",
    ], [{ label: "Delete the record only", value: "delete", danger: true }, { label: "Put the copies back", value: "restore", primary: true }]);
    if (!choice) return;
    try {
      await api("DELETE", gameAPI(`/sales?id=${encodeURIComponent(s.id)}${choice === "restore" ? "&restore=1" : ""}`));
      status(choice === "restore" ? `Put ${what} back in your collection.` : `Deleted the sale of ${what}.`);
      await loadFilters();
      route();
    } catch (e) { status(e.message, true); }
  };
  const gainText = (v) => (v === null ? "gain unknown" : signedUsd(v));
  const gainColor = (v) => (v === null ? null : v >= 0 ? "var(--green)" : "var(--red)");
  const sale = (s) => h("div.sale",
    h("div.sale-main",
      h("span.name", s.card.name),
      h("span.meta", [s.date, sn(s.card), `${fname(s.finish)} ${s.condition}`, `×${s.quantity} at ${usd(s.sale_each)}`,
        s.fees ? `fees ${usd(s.fees)}` : "", s.cost_each ? `paid ${usd(s.cost_each)} each` : "no price paid", s.note].filter(Boolean).join(" · "))),
    h("span.sale-amt", h("b", usd(s.proceeds)), h("small", { style: gainColor(s.gain) ? `color:${gainColor(s.gain)}` : null }, gainText(s.gain)),
      h("button.link", { type: "button", onclick: () => undo(s) }, "Undo…")));
  const months = d.months.length ? h("div.card-box.wide", h("table.months",
    h("thead", h("tr", h("th", "Month"), h("th.n", "Copies"), h("th.n", "Sold for"), h("th.n", "Fees"), h("th.n", "Gain"))),
    h("tbody", [...d.months].reverse().map((m) => h("tr", h("td", m.month), h("td.n", m.copies), h("td.n", usd(m.proceeds)), h("td.n", usd(m.fees)),
      // A month whose sales all lack a price paid has no gain to show.
      m.unknownCostCopies === m.copies ? h("td.n.muted", "unknown") : h("td.n", { style: `color:${gainColor(m.gain)}` }, signedUsd(m.gain))))))) : null;
  fill($("#main"),
    h("div.section-head", h("h1", "Earnings"), years),
    h("div.tiles",
      tile("SOLD FOR", usd(t.proceeds), `${plural(t.copies, "copy", "copies")} in ${plural(t.sales, "sale", "sales")}`),
      tile("FEES", usd(t.fees), ""),
      tile("COST", usd(t.cost), t.unknownCostCopies ? `${plural(t.unknownCostCopies, "copy", "copies")} without a price paid` : "what the sold copies cost"),
      tile("GAIN", t.unknownCostCopies === t.copies && t.copies ? "—" : signedUsd(t.gain), t.unknownCostCopies ? "sales without a price paid are left out" : "sold for, less fees and cost", gainColor(t.gain))),
    months ? h("h2", "By Month") : null, months,
    h("h2", "Sales"),
    d.sales.length ? h("div.card-box", d.sales.map(sale))
      : h("p.muted", year ? `No sales in ${year}.` : "No sales yet. To sell copies, open a card and choose Sell… on the purchase they came from."));
}
