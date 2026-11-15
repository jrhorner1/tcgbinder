//! The watchlist: cards whose price is watched, each with a rule (a move of N% either
//! way from a baseline, or crossing a price). Prices are checked when they refresh;
//! a watch whose price newly crosses its rule is triggered until dismissed or reset.
//! Kept in `watchlist.json` beside the collection.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::collection::{Card, CatalogUpdate, Copies};
use crate::jsonfile::JsonFile;

/// The watchlist file in each game's data folder.
pub const WATCHLIST_FILE: &str = "watchlist.json";

/// When a watch triggers.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Rule {
    /// A move of this many percent either way from the baseline.
    Percent(f64),
    /// The price at or above this.
    Above(f64),
    /// The price at or below this.
    Below(f64),
}

impl Default for Rule {
    fn default() -> Rule {
        Rule::Percent(20.0)
    }
}

impl Rule {
    pub fn validate(&self) -> Result<()> {
        let v = match self {
            Rule::Percent(v) | Rule::Above(v) | Rule::Below(v) => *v,
        };
        if !(v.is_finite() && v > 0.0) {
            bail!("the alert needs a number above 0");
        }
        Ok(())
    }

    /// Whether `price` is past the rule's line, measured from `baseline`.
    pub fn crossed(&self, price: f64, baseline: f64) -> bool {
        match *self {
            Rule::Percent(p) => baseline > 0.0 && ((price - baseline).abs() / baseline * 100.0) >= p - 1e-9,
            Rule::Above(x) => price >= x,
            Rule::Below(x) => price <= x,
        }
    }
}

/// When and at what price a watch triggered.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Trigger {
    pub date: String,
    pub price: f64,
}

/// A watched card in one finish.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Watch {
    /// A catalog copy (no copies), refreshed with the collection's prices.
    pub card: Card,
    pub finish: String,
    /// When it was added (`YYYY-MM-DD`).
    pub added: String,
    /// The price when added, or when last reset; 0 if there was none.
    pub baseline: f64,
    pub baseline_date: String,
    pub rule: Rule,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub triggered: Option<Trigger>,
}

impl Watch {
    /// The watch's key: catalog ID and finish.
    pub fn key(&self) -> String {
        watch_key(&self.card.id, &self.finish)
    }

    /// The current market price in the watched finish.
    pub fn price(&self) -> Option<f64> {
        self.card.market.get(&self.finish).copied()
    }
}

/// A watch's key: "<catalog ID>/<finish>".
pub fn watch_key(id: &str, finish: &str) -> String {
    format!("{id}/{finish}")
}

/// A change to a watch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WatchChange {
    /// A new rule (keeping the baseline and any alert).
    Rule(Rule),
    /// Clears the alert, keeping the baseline: it triggers again only once the price
    /// comes back inside the line and crosses it again.
    Dismiss,
    /// A new baseline at today's price, clearing the alert.
    Reset,
}

/// The watchlist file's contents.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WatchData {
    pub watches: Vec<Watch>,
}

/// One game's watchlist, kept in `watchlist.json` (changed by other devices too,
/// through [`JsonFile`]).
pub struct WatchStore {
    file: JsonFile<WatchData>,
}

impl WatchStore {
    /// Opens (or starts) the watchlist in a game's data folder.
    pub fn open(game_dir: &Path) -> Result<WatchStore> {
        Ok(WatchStore { file: JsonFile::open(game_dir.join(WATCHLIST_FILE), true)? })
    }

    /// Every watch: triggered ones first, then by name.
    pub fn all(&self) -> Vec<Watch> {
        let mut all = self.file.read(|d| d.watches.clone());
        all.sort_by(|a, b| b.triggered.is_some().cmp(&a.triggered.is_some()).then_with(|| a.card.name.cmp(&b.card.name)));
        all
    }

    pub fn get(&self, key: &str) -> Option<Watch> {
        self.file.read(|d| d.watches.iter().find(|w| w.key() == key).cloned())
    }

    /// Starts watching `card` in `finish` from its current price (`date` is today).
    pub fn add(&self, mut card: Card, finish: &str, rule: Rule, date: &str) -> Result<Watch> {
        rule.validate()?;
        card.copies = Copies::default();
        let finish = finish.trim().to_lowercase();
        if card.id.is_empty() || finish.is_empty() {
            bail!("a watch needs a card and a finish");
        }
        let baseline = card.market.get(&finish).copied().unwrap_or(0.0);
        if baseline <= 0.0 && matches!(rule, Rule::Percent(_)) {
            bail!("there's no market price for this finish yet, so a percent change can't be measured; watch for a price instead");
        }
        let watch = Watch { card, finish, added: date.into(), baseline, baseline_date: date.into(), rule, triggered: None };
        self.file.mutate(|d| {
            if d.watches.iter().any(|w| w.key() == watch.key()) {
                bail!("{} is already on the watchlist; change its alert there", watch.card.name);
            }
            d.watches.push(watch.clone());
            Ok(())
        })?;
        Ok(watch)
    }

    /// Changes a watch (`date` is today, for a reset). Returns it as saved.
    pub fn change(&self, key: &str, change: WatchChange, date: &str) -> Result<Watch> {
        if let WatchChange::Rule(r) = change {
            r.validate()?;
        }
        self.file.mutate(|d| {
            let Some(w) = d.watches.iter_mut().find(|w| w.key() == key) else { bail!("that card isn't on the watchlist") };
            match change {
                WatchChange::Rule(r) => w.rule = r,
                WatchChange::Dismiss => w.triggered = None,
                WatchChange::Reset => {
                    w.baseline = w.price().unwrap_or(0.0);
                    w.baseline_date = date.into();
                    w.triggered = None;
                }
            }
            Ok(w.clone())
        })
    }

    pub fn remove(&self, key: &str) -> Result<()> {
        self.file.mutate(|d| {
            d.watches.retain(|w| w.key() != key);
            Ok(())
        })
    }

    /// The watched catalog IDs, for the price refresh.
    pub fn ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.file.read(|d| d.watches.iter().map(|w| w.card.id.clone()).collect());
        ids.sort();
        ids.dedup();
        ids
    }

    /// Applies a price refresh and checks every watch. Returns the watches that newly
    /// triggered: their price was inside the rule's line and now is past it.
    pub fn apply_prices(&self, updates: &HashMap<String, CatalogUpdate>, date: &str) -> Result<Vec<Watch>> {
        if self.file.read(|d| d.watches.iter().all(|w| !updates.contains_key(&w.card.id))) {
            return Ok(Vec::new());
        }
        self.file.mutate(|d| {
            let mut fired = Vec::new();
            for w in &mut d.watches {
                let Some(u) = updates.get(&w.card.id) else { continue };
                let before = w.price();
                w.card.market = u.market.clone();
                w.card.price_from = u.price_from.clone();
                w.card.prices_updated = date.to_string();
                let Some(now) = w.price() else { continue };
                let was = before.unwrap_or(w.baseline);
                if w.triggered.is_none() && w.rule.crossed(now, w.baseline) && !w.rule.crossed(was, w.baseline) {
                    w.triggered = Some(Trigger { date: date.into(), price: now });
                    fired.push(w.clone());
                }
            }
            Ok(fired)
        })
    }

    /// Merges a conflict copy of the watchlist: by card and finish; where both have a
    /// watch, the later `baseline_date` wins. Returns how many watches changed.
    pub fn merge_copy(&self, theirs: WatchData) -> Result<usize> {
        self.file.mutate(|d| {
            let mut changed = 0;
            for w in theirs.watches {
                match d.watches.iter_mut().find(|o| o.key() == w.key()) {
                    Some(ours) if w.baseline_date > ours.baseline_date => {
                        *ours = w;
                        changed += 1;
                    }
                    Some(_) => {}
                    None => {
                        d.watches.push(w);
                        changed += 1;
                    }
                }
            }
            Ok(changed)
        })
    }

    /// Takes in changes another device made.
    pub fn refresh(&self) -> Result<bool> {
        self.file.refresh()
    }

    pub fn path(&self) -> &Path {
        self.file.path()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(id: &str, price: f64) -> Card {
        Card { id: id.into(), name: format!("Card {id}"), market: [("nonfoil".to_string(), price)].into(), ..Card::default() }
    }

    fn price(id: &str, p: f64) -> HashMap<String, CatalogUpdate> {
        HashMap::from([(id.to_string(), CatalogUpdate { market: [("nonfoil".to_string(), p)].into(), ..Default::default() })])
    }

    #[test]
    fn rules() {
        assert!(Rule::Percent(20.0).crossed(12.0, 10.0) && Rule::Percent(20.0).crossed(8.0, 10.0));
        assert!(!Rule::Percent(20.0).crossed(11.99, 10.0));
        assert!(!Rule::Percent(20.0).crossed(5.0, 0.0), "no baseline, no percent");
        assert!(Rule::Above(5.0).crossed(5.0, 1.0) && !Rule::Above(5.0).crossed(4.99, 1.0));
        assert!(Rule::Below(1.0).crossed(0.5, 9.0) && !Rule::Below(1.0).crossed(1.5, 9.0));
        assert!(Rule::Percent(0.0).validate().is_err() && Rule::Above(f64::NAN).validate().is_err());
        let json = serde_json::to_value(Rule::Above(5.5)).unwrap();
        assert_eq!(json, serde_json::json!({"kind": "above", "value": 5.5}));
    }

    #[test]
    fn alerts_fire_once_on_crossing() {
        let dir = tempfile::tempdir().unwrap();
        let s = WatchStore::open(dir.path()).unwrap();
        let w = s.add(card("a", 10.0), "nonfoil", Rule::Percent(20.0), "2026-11-01").unwrap();
        assert_eq!((w.baseline, w.key().as_str()), (10.0, "a/nonfoil"));
        assert!(s.add(card("a", 10.0), "nonfoil", Rule::Above(1.0), "2026-11-01").is_err(), "already watched");
        assert!(s.add(card("b", 0.0), "foil", Rule::Percent(10.0), "2026-11-01").is_err(), "no price to measure from");
        s.add(card("b", 0.0), "foil", Rule::Above(3.0), "2026-11-01").unwrap();
        assert_eq!(s.ids(), ["a", "b"]);

        assert!(s.apply_prices(&price("a", 11.0), "2026-11-02").unwrap().is_empty(), "10% is not 20%");
        let fired = s.apply_prices(&price("a", 7.5), "2026-11-03").unwrap();
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].triggered, Some(Trigger { date: "2026-11-03".into(), price: 7.5 }));
        assert!(s.apply_prices(&price("a", 7.0), "2026-11-04").unwrap().is_empty(), "already triggered");
        assert_eq!(s.all()[0].card.id, "a", "triggered first");

        // Dismissed while still past the line: no new alert until it comes back and crosses again.
        s.change("a/nonfoil", WatchChange::Dismiss, "2026-11-05").unwrap();
        assert!(s.apply_prices(&price("a", 6.0), "2026-11-05").unwrap().is_empty());
        assert!(s.apply_prices(&price("a", 10.0), "2026-11-06").unwrap().is_empty());
        assert_eq!(s.apply_prices(&price("a", 13.0), "2026-11-07").unwrap().len(), 1);

        // Reset: a new baseline at today's price.
        let reset = s.change("a/nonfoil", WatchChange::Reset, "2026-11-08").unwrap();
        assert_eq!((reset.baseline, reset.baseline_date.as_str(), reset.triggered.is_none()), (13.0, "2026-11-08", true));
        let ruled = s.change("a/nonfoil", WatchChange::Rule(Rule::Below(12.0)), "2026-11-08").unwrap();
        assert_eq!(ruled.rule, Rule::Below(12.0));
        assert!(s.change("a/nonfoil", WatchChange::Rule(Rule::Below(-1.0)), "x").is_err());
        assert!(s.change("nope", WatchChange::Dismiss, "x").is_err());
        s.remove("a/nonfoil").unwrap();
        assert_eq!(s.all().len(), 1);
    }

    #[test]
    fn conflict_merge_keeps_the_later_baseline() {
        let dir = tempfile::tempdir().unwrap();
        let s = WatchStore::open(dir.path()).unwrap();
        let ours = s.add(card("a", 10.0), "nonfoil", Rule::Percent(20.0), "2026-11-01").unwrap();
        let newer = Watch { baseline: 12.0, baseline_date: "2026-11-09".into(), ..ours.clone() };
        let older = Watch { baseline: 1.0, baseline_date: "2026-01-01".into(), ..ours.clone() };
        let other = Watch { card: card("b", 1.0), finish: "nonfoil".into(), ..ours.clone() };
        assert_eq!(s.merge_copy(WatchData { watches: vec![newer, other] }).unwrap(), 2);
        assert_eq!(s.get("a/nonfoil").unwrap().baseline, 12.0);
        assert_eq!(s.merge_copy(WatchData { watches: vec![older] }).unwrap(), 0);
        assert_eq!(s.all().len(), 2);
    }
}
