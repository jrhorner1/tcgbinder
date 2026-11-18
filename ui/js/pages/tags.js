// The Tags page: every tag with its totals, renaming and deleting one.

import { api, gameAPI, loadFilters } from "../api.js";
import { sameTag, tagProblem, tagStyle, tidyTag } from "../cards.js";
import { $, confirmDialog, fill, h, status } from "../dom.js";
import { plural, usd } from "../format.js";
import { route } from "../nav.js";

// viewTags is the Tags page: every tag with what carries it, to rename or delete.
export async function viewTags() {
  const d = await api("GET", gameAPI("/tags"));
  const done = async (msg, r) => { status(msg, false, r.undo); await loadFilters(); route(); };
  const rename = async (t) => {
    const input = h("input", { type: "text", value: t.name, "aria-label": "New name" });
    const problem = h("p.error");
    const name = await new Promise((resolve) => {
      const ok = () => { const v = tidyTag(input.value); const why = tagProblem(v); if (why) { problem.textContent = why; return; } resolve(v); dlg.close(); };
      input.addEventListener("keydown", (e) => { if (e.key === "Enter") ok(); });
      const dlg = h("dialog.ask", h("div.dlg", h("header", h("h2", `Rename “${t.name}”`)),
        h("div.body", input, h("p.note", `It changes on ${plural(t.cards, "card", "cards")} (${plural(t.copies, "copy", "copies")}).`), problem),
        h("footer", h("button", { type: "button", onclick: () => { resolve(null); dlg.close(); } }, "Cancel"), h("button.primary", { type: "button", onclick: ok }, "Rename"))));
      dlg.addEventListener("close", () => { resolve(null); dlg.remove(); });
      document.body.append(dlg);
      dlg.showModal();
      input.select();
    });
    if (!name || name === t.name) return;
    const other = d.tags.find((x) => sameTag(x.name, name) && !sameTag(x.name, t.name));
    if (other && !(await confirmDialog(`“${other.name}” is already a tag. The two will be combined into one, “${other.name}”, on every card.`,
      { action: "Combine", title: "Combine the tags?" }))) return;
    try {
      const r = await api("PUT", gameAPI(`/tags?name=${encodeURIComponent(t.name)}`), { name });
      await done(other ? `Combined “${t.name}” into “${other.name}”.` : `Renamed “${t.name}” to “${name}”.`, r);
    } catch (e) { status(e.message, true); }
  };
  const remove = async (t) => {
    if (!(await confirmDialog(`Remove the tag “${t.name}” from ${plural(t.cards, "card", "cards")} (${plural(t.copies, "copy", "copies")})? The cards stay in your collection.`,
      { danger: true, action: "Delete tag", title: "Delete the tag?" }))) return;
    try {
      const r = await api("DELETE", gameAPI(`/tags?name=${encodeURIComponent(t.name)}`));
      await done(`Deleted the tag “${t.name}”.`, r);
    } catch (e) { status(e.message, true); }
  };
  // A row of the table; on a phone each row stacks, its figures labelled.
  const cells = (t) => [
    h("td.n", { "data-label": "Cards" }, t.cards.toLocaleString()), h("td.n", { "data-label": "Copies" }, t.copies.toLocaleString()),
    h("td.n", { "data-label": "Value" }, usd(t.value)), h("td.n", { "data-label": "Paid" }, usd(t.paid))];
  const row = (t) => h("tr",
    h("td.tag-name", h("a.tag-chip", { href: `#/collection?${new URLSearchParams({ tags: t.name })}`, style: tagStyle(t.name) }, t.name)),
    cells(t),
    h("td.actions", h("button", { type: "button", onclick: () => rename(t) }, "Rename…"), " ", h("button", { type: "button", onclick: () => remove(t) }, "Delete…")));
  const table = h("table.tags-table",
    h("thead", h("tr", h("th", "Tag"), h("th.n", "Cards"), h("th.n", "Copies"), h("th.n", "Market value"), h("th.n", "Price paid"), h("th", h("span.sr", "Actions")))),
    h("tbody", d.tags.map(row),
      d.untagged ? h("tr.untagged", h("td.tag-name", h("a", { href: "#/collection?untagged=1" }, "Untagged")), cells(d.untagged), h("td.actions")) : null));
  fill($("#main"),
    h("h1", "Tags"),
    h("p.muted", "Tags are labels on your copies, such as where they're kept or which deck they're in. Add them on a card's page, when adding cards, with Bulk edit, or to a whole import. Totals count only the tagged copies."),
    d.tags.length || d.untagged ? h("div.card-box", table) : h("p.muted", "No cards yet."),
    !d.tags.length && d.untagged ? h("p.muted", "No tags yet. Open a card and choose Add tags on one of its purchases.") : null);
}
