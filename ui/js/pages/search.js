// Search results across every game, from the search box in the sidebar.

import { api, loadFilters } from "../api.js";
import { copiesSummary } from "../cards.js";
import { $, fill, h, store } from "../dom.js";
import { plural, sn } from "../format.js";
import { go } from "../nav.js";
import { state } from "../state.js";

// Search: owned cards matching q in every game, plus a catalog search to add more.
export async function viewSearch(params) {
  const q = new URLSearchParams(params).get("q") || "";
  $("#gsq").value = q;
  const r = await api("GET", `/api/search?q=${encodeURIComponent(q)}`);
  const total = r.games.reduce((n, g) => n + g.Total, 0);
  const switchTo = async (id) => {
    if (state.game !== id) { state.game = id; store("game", id); $("#game").value = id; await loadFilters(); }
  };
  fill($("#main"), h("h1", `Search: “${q}”`),
    total ? null : h("p.muted", "No cards in your collection match. Search the catalog to add one."),
    r.games.map((g) => [
      r.games.length > 1 || g.Total ? h("h2", `${g.Name} — ${plural(g.Total, "match", "matches")}`) : null,
      g.Cards.length ? h("div.list", g.Cards.map((c) => h("button.card", { type: "button", onclick: async () => { await switchTo(g.Game); go("collection", new URLSearchParams({ key: c.key })); } },
        h("span.text", h("span.name", c.name), h("span.meta", `${sn(c)} · ${copiesSummary(c)}`))))) : null,
      g.Total > g.Cards.length ? h("p", h("span.muted", `Showing the first ${g.Cards.length}. `),
        h("button.link", { type: "button", onclick: async () => { await switchTo(g.Game); go("collection", new URLSearchParams({ q })); } }, `Show all ${g.Total.toLocaleString()}`)) : null,
      h("p", h("button", { type: "button", onclick: async () => { await switchTo(g.Game); go("add", new URLSearchParams({ text: q })); } },
        `Search the ${g.Name} catalog for “${q}”`)),
    ]));
}
