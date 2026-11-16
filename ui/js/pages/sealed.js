// Sealed product: boxes, bundles and decks kept unopened, valued by hand.

import { api, gameAPI } from "../api.js";
import { gain, tile } from "../charts.js";
import { $, ask, fill, h, status } from "../dom.js";
import { plural, signedUsd, title, today, usd } from "../format.js";
import { route } from "../nav.js";
import { state } from "../state.js";

// productKind names a sealed product's category and subtype: "Booster box · Draft".
const productKind = (p) => [p.category, p.subtype !== "default" ? p.subtype : ""].filter(Boolean).map((x) => title(x.replaceAll("_", " "))).join(" · ");

// showSealed adds a sealed item, or edits `item`.
function showSealed(item) {
  const it = item || {};
  const dlg = h("dialog.ask");
  let chosen = it.product_uuid ? { uuid: it.product_uuid, category: it.category, subtype: it.subtype, tcgplayer_product_id: it.tcgplayer_product_id, purchase_url: it.purchase_url } : null;
  const set = h("input", { type: "text", value: it.set || "", placeholder: "Set code, e.g. M19", "aria-label": "Set code", autocapitalize: "characters" });
  const find = h("button", { type: "button" }, "Find products");
  const pick = h("select", { "aria-label": "Product", hidden: true });
  const name = h("input", { type: "text", value: it.name || "", placeholder: "Product name", "aria-label": "Product name" });
  const qty = h("input", { type: "number", min: 1, step: 1, value: it.quantity || 1, "aria-label": "Quantity" });
  const paid = h("input", { type: "number", min: 0, step: "0.01", value: it.price_paid || "", placeholder: "Each", "aria-label": "Price paid each" });
  const date = h("input", { type: "date", value: it.acquired_date || (state.settings.default_date === "today" ? today() : ""), "aria-label": "Acquired" });
  const value = h("input", { type: "number", min: 0, step: "0.01", value: it.value || "", placeholder: "Each, as you look it up", "aria-label": "Value each" });
  const note = h("input", { type: "text", value: it.note || "", placeholder: "Where it's kept, condition…", "aria-label": "Note" });
  const found = h("p.note");
  const err = h("p.error");
  let products = [];
  const choose = () => {
    const p = products[parseInt(pick.value, 10)];
    chosen = p || null;
    if (p) name.value = p.name;
    else { name.value = ""; name.focus(); }
  };
  pick.addEventListener("change", choose);
  find.addEventListener("click", async () => {
    err.textContent = "";
    if (!set.value.trim()) { err.textContent = "Type the set's code first."; set.focus(); return; }
    find.disabled = true;
    found.textContent = "Looking up the set's products…";
    try {
      products = (await api("GET", gameAPI(`/sealed/products?set=${encodeURIComponent(set.value.trim())}`))).products;
      fill(pick, h("option", { value: "" }, products.length ? "Choose a product, or type a name below" : "Type a name below"),
        products.map((p, i) => h("option", { value: i }, `${p.name} (${productKind(p)})`)));
      pick.hidden = !products.length;
      found.textContent = products.length ? `${plural(products.length, "product", "products")} in ${set.value.trim().toUpperCase()}.` : `No products listed for ${set.value.trim().toUpperCase()}; type the name.`;
      if (products.length) pick.focus();
    } catch (e) { found.textContent = ""; err.textContent = e.message; }
    find.disabled = false;
  });
  name.addEventListener("input", () => { if (chosen && name.value !== (products.find((p) => p.uuid === chosen.uuid) || {}).name && !item) chosen = null; });
  const close = () => { dlg.close(); dlg.remove(); };
  const save = async () => {
    err.textContent = "";
    const body = {
      name: name.value.trim(), set: set.value.trim(), quantity: parseInt(qty.value, 10), price_paid: parseFloat(paid.value) || 0,
      acquired_date: date.value, value: parseFloat(value.value) || 0, value_date: it.value_date || "", // the server dates a changed value today
      note: note.value.trim(), product_uuid: chosen ? chosen.uuid : "", category: chosen ? chosen.category : "", subtype: chosen ? chosen.subtype : "",
      tcgplayer_product_id: chosen ? chosen.tcgplayer_product_id || 0 : 0, purchase_url: chosen ? chosen.purchase_url || "" : "",
    };
    if (!body.name) { err.textContent = "Choose a product or type its name."; name.focus(); return; }
    if (!(body.quantity >= 1)) { err.textContent = "The quantity must be at least 1."; qty.focus(); return; }
    try {
      if (item) await api("PUT", gameAPI(`/sealed?id=${encodeURIComponent(item.id)}`), body);
      else await api("POST", gameAPI("/sealed"), body);
      close();
      status(item ? `Saved ${body.name}.` : `Added ${plural(body.quantity, "item", "items")}: ${body.name}.`);
      route();
    } catch (e) { err.textContent = e.message; }
  };
  fill(dlg, h("div.dlg", h("header", h("h2", item ? "Edit sealed product" : "Add sealed product")),
    h("div.body",
      h("div.form",
        h("label", "Set"), h("div.inline", set, find),
        h("label", "Product"), h("div.stack", pick, name),
        h("label", "Quantity"), qty, h("label", "Price paid (each)"), paid, h("label", "Acquired"), date,
        h("label", "Value (each)"), value, h("label", "Note"), note),
      found,
      h("p.note", "There's no free price source for sealed product, so the value is yours to enter: look it up (the product's link on the Sealed page helps) and update it now and then. Values older than 90 days are marked."),
      err),
    h("footer", h("button", { type: "button", onclick: close }, "Cancel"), h("button.primary", { type: "button", onclick: save }, item ? "Save" : "Add"))));
  dlg.addEventListener("close", () => dlg.remove());
  document.body.append(dlg);
  dlg.showModal();
  (item ? value : set).focus();
}

// viewSealed lists sealed product with what was paid and what it's worth.
export async function viewSealed() {
  const d = await api("GET", gameAPI("/sealed"));
  const t = d.totals;
  // Gain over the items with both a price paid and a value: the others would skew it.
  const both = d.items.filter((i) => i.gain !== null);
  const g = both.length ? gain(both.reduce((n, i) => n + i.price_paid * i.quantity, 0), both.reduce((n, i) => n + i.value * i.quantity, 0)) : null;
  const left = d.items.length - both.length;
  const remove = async (i) => {
    if (!(await ask(`Remove ${i.name} from your sealed product?`, "Remove"))) return;
    try { await api("DELETE", gameAPI(`/sealed?id=${encodeURIComponent(i.id)}`)); status(`Removed ${i.name}.`); route(); } catch (e) { status(e.message, true); }
  };
  const row = (i) => h("div.list-entry" + (i.valueStale ? ".stale" : ""),
    h("div.text",
      h("span.name", i.name),
      h("span.meta", [i.set, productKind(i), `×${i.quantity}`, i.price_paid ? `paid ${usd(i.price_paid)} each` : "no price paid", i.acquired_date, i.note].filter(Boolean).join(" · ")),
      h("span.meta", i.value ? `worth ${usd(i.value)} each, as of ${i.value_date || "—"}${i.valueStale ? " · over 90 days old: check it" : ""}` : "no value yet: look it up and enter it",
        i.purchase_url ? [" · ", h("a", { href: i.purchase_url, target: "_blank", rel: "noopener noreferrer" }, "Look up a price ↗")] : null)),
    h("div.entry-controls",
      h("span.change" + (i.gain > 0 ? ".up" : i.gain < 0 ? ".down" : ""), i.gain === null ? (i.value ? usd(i.value * i.quantity) : "—") : signedUsd(i.gain)),
      h("button", { type: "button", onclick: () => showSealed(i) }, "Edit…"),
      h("button", { type: "button", "aria-label": `Remove ${i.name}`, title: "Remove", onclick: () => remove(i) }, "✕")));
  fill($("#main"),
    h("div.section-head", h("h1", "Sealed product"), h("button.primary", { type: "button", onclick: () => showSealed() }, "Add sealed product")),
    h("p.muted", "Boxes, bundles and decks you keep unopened. Values are entered by hand (no free source prices sealed product) and aren't part of the cards' market value on Home."),
    h("div.tiles",
      tile("ITEMS", t.items.toLocaleString(), t.unvalued ? `${t.unvalued} without a value` : ""),
      tile("PRICE PAID", usd(t.paid), ""),
      tile("VALUE", usd(t.value), t.stale ? `${plural(t.stale, "value", "values")} to enter or check` : "every value is recent", "var(--blue)"),
      tile("GAIN / LOSS", g ? g.text : "—", g ? [g.pct, left ? `${plural(left, "item", "items")} without a price paid or value left out` : ""].filter(Boolean).join(" · ") : "needs prices paid and values", g ? g.color : null)),
    d.items.length ? h("div.card-box", d.items.map(row)) : h("p.muted", "No sealed product yet."));
}
