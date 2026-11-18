// The Statistics page: breakdowns, value bands, tags and most copies.

import { api, gameAPI } from "../api.js";
import { cardRows } from "../cards.js";
import { bars } from "../charts.js";
import { $, fill, h } from "../dom.js";
import { choiceNames, fname, languageName, plural, title, usd } from "../format.js";
import { go } from "../nav.js";

// statLabel names a statistics row: a rarity, finish, condition, language or color.
function statLabel(id, key) {
  switch (id) {
    case "rarity": return title(key || "unknown");
    case "finish": return fname(key);
    case "condition": return choiceNames.default_condition[key] || key;
    case "language": return languageName(key);
    case "Color": return { W: "White", U: "Blue", B: "Black", R: "Red", G: "Green" }[key] || key;
    default: return key;
  }
}

// viewStats is the Statistics page.
export async function viewStats() {
  const s = await api("GET", gameAPI("/stats"));
  const filter = (params) => () => go("collection", new URLSearchParams(params));
  const breakdown = (t) => {
    const max = Math.max(1, ...t.rows.map((r) => r.cards));
    return h("div.card-box",
      h("h2", { style: "margin-top:0" }, t.name),
      t.rows.length ? bars(t.rows.map((r) => ({
        label: statLabel(t.id, r.key),
        value: r.cards,
        text: `${r.cards.toLocaleString()} · ${usd(r.value)}`,
        onclick: t.param ? filter({ [t.param]: r.key }) : null,
      })), max, "var(--blue)") : h("p.muted", "No cards."));
  };
  const bandMax = Math.max(0.01, ...s.bands.map((b) => b.value));
  const bands = bars(s.bands.map((b) => {
    const params = {};
    if (b.min !== null) params.price_min = b.min;
    if (b.max !== null) params.price_max = (b.max - 0.01).toFixed(2);
    return {
      label: b.label,
      value: b.value,
      text: `${b.cards.toLocaleString()} · ${usd(b.value)} · ${Math.round(b.share * 100)}%`,
      onclick: b.label === "No price" ? filter({ unpriced: "1" }) : filter(params),
    };
  }), bandMax, "var(--amber)");
  // Tags count only the tagged copies, so they aren't a breakdown of whole cards.
  const tagsBox = (t) => {
    if (!t.tags.length) return null;
    const max = Math.max(1, ...t.tags.map((x) => x.copies));
    const rows = t.tags.map((x) => ({ label: x.name, value: x.copies, text: `${x.copies.toLocaleString()} · ${usd(x.value)}`,
      onclick: filter({ tags: x.name }) }));
    if (t.untagged) rows.push({ label: "Untagged", value: t.untagged.copies, text: `${t.untagged.copies.toLocaleString()} · ${usd(t.untagged.value)}`, onclick: filter({ untagged: "1" }) });
    return h("div.card-box", h("h2", { style: "margin-top:0" }, "Tags"), bars(rows, Math.max(max, t.untagged ? t.untagged.copies : 0), "var(--blue)"),
      h("p.note", "Copies carrying each tag, and their market value. A copy with two tags counts under both."));
  };
  const months = [...s.byMonth].reverse();
  const monthTable = (rows) => h("table.months",
    h("thead", h("tr", h("th", "Month"), h("th.n", "Copies"), h("th.n", "Spent"))),
    h("tbody", rows.map((m) => h("tr", h("td", m.month), h("td.n", m.cards.toLocaleString()), h("td.n", usd(m.paid))))));
  const monthBox = h("div.card-box.wide");
  const showMonths = (all) => fill(monthBox, monthTable(all ? months : months.slice(0, 12)),
    !all && months.length > 12 ? h("button.link", { type: "button", onclick: () => showMonths(true) }, `Show all ${months.length} months`) : null);
  showMonths(false);
  fill($("#main"),
    h("h1", "Statistics"),
    h("p.muted", "Counts are copies. Each bar's figures are copies and market value (and, by price, the share of the collection's value). Click a row to see those cards in the collection list."),
    h("h2", "By Price of One Copy"),
    h("div.card-box", bands),
    h("div.two-col.stats", s.breakdowns.map(breakdown), tagsBox(s.tags)),
    h("div.two-col",
      h("div", h("h2", "By Month Acquired"),
        months.length ? monthBox : h("p.muted", "No acquired dates recorded."),
        s.undated ? h("p.note", `${plural(s.undated, "copy has", "copies have")} no acquired date and aren't counted here.`) : null),
      h("div", h("h2", "Most Copies"),
        s.mostCopies.length ? h("div.card-box", cardRows(s.mostCopies, (c) => c.market_value)) : h("p.muted", "No card has more than one copy."))));
}
