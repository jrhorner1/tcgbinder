//! Lists and decks (see `tcgbinder_core::lists`).

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{Value, json};

use tcgbinder_core::collection::{Card, round_cents};
use tcgbinder_core::lists::{List, ListEntry};
use tcgbinder_core::service::GameService;

use crate::views::view;
use crate::{Api, Reply, Result, bad, decode, internal, not_found};

/// An entry as a request sends it.
#[derive(Deserialize)]
#[serde(default)]
struct EntryForm {
    card: Card,
    quantity: i64,
    any_printing: bool,
    finish: String,
}

impl Default for EntryForm {
    fn default() -> EntryForm {
        EntryForm { card: Card::default(), quantity: 1, any_printing: false, finish: String::new() }
    }
}

impl EntryForm {
    fn entry(self) -> Result<ListEntry> {
        if self.card.id.is_empty() || self.quantity <= 0 {
            return Err(bad("an entry needs a card and a quantity of at least 1"));
        }
        Ok(ListEntry::new(self.card, self.quantity, self.any_printing, &self.finish))
    }
}

/// Totals for a list against the collection: `cards` (copies wanted), `owned` (of
/// those), `missing`, `marketValue` (every copy wanted) and `missingCost` (the
/// missing copies at market price).
fn totals(g: &GameService, list: &List, collection: &[Card]) -> Value {
    let order = g.game.finishes();
    let (mut wanted, mut owned, mut value, mut missing_cost) = (0, 0, 0.0, 0.0);
    for e in &list.entries {
        let have = e.owned(collection).min(e.quantity);
        let price = e.price(order).unwrap_or(0.0);
        wanted += e.quantity;
        owned += have;
        value += price * e.quantity as f64;
        missing_cost += price * (e.quantity - have) as f64;
    }
    json!({
        "id": list.id, "name": list.name, "notes": list.notes, "created": list.created, "entries": list.entries.len(),
        "cards": wanted, "owned": owned, "missing": wanted - owned,
        "marketValue": round_cents(value), "missingCost": round_cents(missing_cost),
    })
}

/// A list's decklist text: "4 Lightning Bolt (M11) 149", with "*F*" or "*E*" for a
/// foil or etched entry; an any-printing entry is just the count and name.
fn decklist(list: &List) -> String {
    let mut out = String::new();
    for e in &list.entries {
        let c = &e.card;
        out.push_str(&format!("{} {}", e.quantity, c.name));
        if !e.any_printing && !c.set.is_empty() {
            out.push_str(&format!(" ({})", c.set.to_uppercase()));
            if !c.collector_number.is_empty() {
                out.push_str(&format!(" {}", c.collector_number));
            }
        }
        match e.finish.as_str() {
            "foil" => out.push_str(" *F*"),
            "etched" => out.push_str(" *E*"),
            _ => {}
        }
        out.push('\n');
    }
    out
}

impl Api {
    fn find_list(g: &GameService, params: &HashMap<String, String>) -> Result<List> {
        let id = params.get("id").map_or("", String::as_str);
        g.lists.get(id).ok_or_else(|| not_found(format!("there is no list {id:?}")))
    }

    /// Every list with its totals.
    pub(crate) fn lists(&self, g: &GameService) -> Reply {
        let collection = g.store.cards();
        Reply::Json(json!(g.lists.all().iter().map(|l| totals(g, l, &collection)).collect::<Vec<_>>()))
    }

    /// One list: its totals and entries (`index`, `card`, `quantity`, `any_printing`,
    /// `finish`, `owned`, `price`).
    pub(crate) fn list(&self, g: &GameService, params: &HashMap<String, String>) -> Result {
        let list = Self::find_list(g, params)?;
        let collection = g.store.cards();
        let order = g.game.finishes();
        let entries: Vec<Value> = list
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| {
                json!({
                    "index": i, "card": view(&e.card), "quantity": e.quantity, "any_printing": e.any_printing, "finish": e.finish,
                    "owned": e.owned(&collection), "price": e.price(order),
                })
            })
            .collect();
        Ok(Reply::Json(json!({ "list": totals(g, &list, &collection), "entries": entries })))
    }

    /// Creates a list: `{"name", "notes"}`.
    pub(crate) fn create_list(&self, g: &GameService, body: &[u8]) -> Result {
        #[derive(Deserialize)]
        #[serde(default)]
        #[derive(Default)]
        struct Req {
            name: String,
            notes: String,
        }
        let req: Req = decode(body)?;
        let list =
            g.lists.add(List { name: req.name, notes: req.notes.trim().to_string(), ..List::default() }).map_err(|e| bad(e.to_string()))?;
        tracing::info!(game = g.game.id(), list = list.name, "list created");
        Ok(Reply::Json(totals(g, &list, &g.store.cards())))
    }

    /// Changes a list: any of `name`, `notes` and `entries` (which replaces them all).
    pub(crate) fn update_list(&self, g: &GameService, params: &HashMap<String, String>, body: &[u8]) -> Result {
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Req {
            name: Option<String>,
            notes: Option<String>,
            entries: Option<Vec<EntryForm>>,
        }
        let mut list = Self::find_list(g, params)?;
        let req: Req = decode(body)?;
        if let Some(name) = req.name {
            list.name = name;
        }
        if let Some(notes) = req.notes {
            list.notes = notes.trim().to_string();
        }
        if let Some(entries) = req.entries {
            list.entries = entries.into_iter().map(EntryForm::entry).collect::<Result<_>>()?;
        }
        let list = g.lists.save(list).map_err(|e| bad(e.to_string()))?;
        Ok(Reply::Json(totals(g, &list, &g.store.cards())))
    }

    pub(crate) fn delete_list(&self, g: &GameService, params: &HashMap<String, String>) -> Result {
        let list = Self::find_list(g, params)?;
        g.lists.remove(&list.id).map_err(internal)?;
        tracing::info!(game = g.game.id(), list = list.name, "list deleted");
        Ok(Reply::Json(json!({ "deleted": list.name })))
    }

    /// Adds a catalog card to a list: `{"card", "quantity", "any_printing", "finish"}`.
    pub(crate) fn add_list_entry(&self, g: &GameService, params: &HashMap<String, String>, body: &[u8]) -> Result {
        let list = Self::find_list(g, params)?;
        let form: EntryForm = decode(body)?;
        let entry = form.entry()?;
        let name = entry.card.name.clone();
        let list = g.lists.add_entries(&list.id, vec![entry]).map_err(|e| bad(e.to_string()))?;
        tracing::info!(game = g.game.id(), list = list.name, card = name, "added to list");
        Ok(Reply::Json(totals(g, &list, &g.store.cards())))
    }

    /// Downloads a list as decklist text.
    pub(crate) fn export_list(&self, g: &GameService, params: &HashMap<String, String>) -> Result {
        let list = Self::find_list(g, params)?;
        let stem: String = list.name.chars().map(|c| if c.is_alphanumeric() || c == '-' { c } else { '-' }).collect();
        Ok(Reply::File { name: format!("{stem}.txt"), content_type: "text/plain; charset=utf-8", data: decklist(&list).into_bytes() })
    }

    /// Adds a pasted card list (or an import file) to a list. Lines matched by name
    /// only count any printing. Returns the list's totals, `added` and the import
    /// `notes` and `unmatched` lines.
    pub(crate) fn import_list(&self, g: &GameService, params: &HashMap<String, String>, body: &[u8]) -> Result {
        let list = Self::find_list(g, params)?;
        self.job(g, "list-import", |r| {
            let (entries, report) = g.list_entries(body, &mut |done, total| r.report(done, total, "Matching cards"))?;
            let added: i64 = entries.iter().map(|e| e.quantity).sum();
            let list = g.lists.add_entries(&list.id, entries)?;
            tracing::info!(game = g.game.id(), list = list.name, added, "card list added to list");
            Ok(json!({
                "list": totals(g, &list, &g.store.cards()), "added": added, "notes": report.notes,
                "unmatched": report.unmatched.iter().map(|u| u.text.clone()).collect::<Vec<_>>(),
            }))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decklist_lines() {
        let card = |name: &str, set: &str, num: &str| Card {
            id: name.into(),
            name: name.into(),
            set: set.into(),
            collector_number: num.into(),
            ..Card::default()
        };
        let list = List {
            entries: vec![
                ListEntry::new(card("Lightning Bolt", "m11", "149"), 4, false, ""),
                ListEntry::new(card("Opt", "dom", "60"), 2, true, ""),
                ListEntry::new(card("Shock", "m19", "156"), 1, false, "foil"),
            ],
            ..List::default()
        };
        assert_eq!(decklist(&list), "4 Lightning Bolt (M11) 149\n2 Opt\n1 Shock (M19) 156 *F*\n");
    }
}
