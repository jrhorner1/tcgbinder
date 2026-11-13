use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use super::Copies;

/// The physical state of a card. The grades are shared by most TCGs. They order from
/// best to worst, so maps keyed by condition list near mint first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum Condition {
    #[default]
    #[serde(rename = "NM")]
    NearMint,
    #[serde(rename = "LP")]
    LightPlay,
    #[serde(rename = "MP")]
    Moderate,
    #[serde(rename = "HP")]
    HeavyPlay,
    #[serde(rename = "DMG")]
    Damaged,
}

impl Condition {
    /// Every condition, best first.
    pub const ALL: [Condition; 5] =
        [Condition::NearMint, Condition::LightPlay, Condition::Moderate, Condition::HeavyPlay, Condition::Damaged];

    /// The short code stored and shown, e.g. "NM".
    pub fn code(self) -> &'static str {
        match self {
            Condition::NearMint => "NM",
            Condition::LightPlay => "LP",
            Condition::Moderate => "MP",
            Condition::HeavyPlay => "HP",
            Condition::Damaged => "DMG",
        }
    }

    /// Normalizes condition labels (TCGplayer, Cardmarket and abbreviations). Anything
    /// unrecognized, or nothing, is near mint.
    pub fn parse(raw: &str) -> Condition {
        let spaced = raw.replace(['_', '-'], " ");
        let c = spaced.split_whitespace().collect::<Vec<_>>().join(" ").to_uppercase();
        let has = |s: &str| c.contains(s);
        if c.is_empty() {
            Condition::NearMint
        } else if c == "DMG" || c == "D" || c == "PO" || has("DAMAGED") || has("POOR") {
            Condition::Damaged
        } else if c == "HP" || has("HEAVY") || has("HEAVILY") {
            Condition::HeavyPlay
        } else if c == "MP" || has("MODERATE") || c == "GD" || c == "GOOD" || c == "PL" || c == "PLAYED" {
            Condition::Moderate
        } else if c == "LP" || c == "SP" || c == "EX" || has("LIGHT") || has("SLIGHT") || has("EXCELLENT") {
            Condition::LightPlay
        } else {
            Condition::NearMint
        }
    }
}

impl fmt::Display for Condition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

// Conditions are read leniently: files and requests may spell them any way parse knows.
impl<'de> Deserialize<'de> for Condition {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Condition::parse(&String::deserialize(d)?))
    }
}

/// A card in the collection: one printing, with the copies owned grouped by finish,
/// then condition ([`Copies`]; the card editor's "Your Copies"). The fields here are
/// common to every game; anything a particular game adds (Magic's mana cost, colors,
/// Oracle ID, ...) lives in `attrs`, stored as the card's "attrs" object.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Card {
    /// The game catalog's printing ID (a Scryfall ID for Magic).
    pub id: String,
    pub name: String,
    /// Set code, uppercase.
    pub set: String,
    pub set_name: String,
    pub collector_number: String,
    pub rarity: String,
    pub released_at: String,
    pub image_uri: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub type_line: String,
    /// The rules text.
    #[serde(rename = "oracle_text", skip_serializing_if = "String::is_empty")]
    pub rules_text: String,
    #[serde(rename = "lang")]
    pub language: String,
    /// The variants the printing exists in (e.g. nonfoil, foil).
    pub finishes: Vec<String>,
    /// The copies owned: finish → condition → purchases.
    #[serde(skip_serializing_if = "Copies::is_empty")]
    pub copies: Copies,
    /// USD market price per finish as of `prices_updated`.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub market: BTreeMap<String, f64>,
    /// `YYYY-MM-DD` of the last price refresh.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub prices_updated: String,
    /// Game-specific fields, keyed by their JSON name. Use the attr helpers.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub attrs: BTreeMap<String, Value>,
}

impl Card {
    /// A game-specific field, if set.
    pub fn attr(&self, key: &str) -> Option<&Value> {
        self.attrs.get(key)
    }

    /// A string attribute ("" if missing or not a string).
    pub fn attr_str(&self, key: &str) -> &str {
        self.attrs.get(key).and_then(Value::as_str).unwrap_or("")
    }

    /// A string-list attribute (empty if missing).
    pub fn attr_strings(&self, key: &str) -> Vec<&str> {
        match self.attrs.get(key) {
            Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).collect(),
            _ => Vec::new(),
        }
    }

    /// Stores a game-specific field. `Value::Null` removes it.
    pub fn set_attr(&mut self, key: &str, value: impl Into<Value>) {
        match value.into() {
            Value::Null => {
                self.attrs.remove(key);
            }
            v => {
                self.attrs.insert(key.to_string(), v);
            }
        }
    }

    /// The current per-copy USD market price in a finish, from the prices stored at
    /// the last refresh; `None` when the catalog had no price.
    pub fn market_price(&self, finish: &str) -> Option<f64> {
        self.market.get(finish).copied()
    }
}

/// Fresh catalog data for one printing, from the game's price refresh.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CatalogUpdate {
    /// USD per finish.
    pub market: BTreeMap<String, f64>,
    pub type_line: String,
    pub rules_text: String,
}

/// A set, for completion stats.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SetInfo {
    pub code: String,
    pub name: String,
    pub card_count: usize,
    pub released_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_condition() {
        for (raw, want) in [
            ("", Condition::NearMint),
            ("Near Mint", Condition::NearMint),
            ("NM-M", Condition::NearMint),
            ("Lightly Played", Condition::LightPlay),
            ("slightly_played", Condition::LightPlay),
            ("EX", Condition::LightPlay),
            ("Moderately Played", Condition::Moderate),
            ("played", Condition::Moderate),
            ("Heavily Played", Condition::HeavyPlay),
            ("Damaged", Condition::Damaged),
            ("poor", Condition::Damaged),
            ("whatever", Condition::NearMint),
        ] {
            assert_eq!(Condition::parse(raw), want, "{raw:?}");
        }
    }

    #[test]
    fn attrs() {
        let mut c = Card::default();
        c.set_attr("colors", vec!["R", "G"]);
        c.set_attr("mana_cost", "{R}");
        assert_eq!(c.attr_strings("colors"), ["R", "G"]);
        assert_eq!(c.attr_str("mana_cost"), "{R}");
        assert_eq!(c.attr_str("missing"), "");
        c.set_attr("mana_cost", Value::Null);
        assert!(c.attr("mana_cost").is_none());
    }

    #[test]
    fn json_names() {
        let mut c = Card { id: "x".into(), rules_text: "Draw a card.".into(), language: "en".into(), ..Card::default() };
        c.copies.add("nonfoil", Condition::LightPlay, super::super::Purchase { quantity: 2, ..Default::default() });
        let v = serde_json::to_value(&c).unwrap();
        assert_eq!(v["oracle_text"], "Draw a card.");
        assert_eq!(v["lang"], "en");
        assert_eq!(v["copies"]["nonfoil"]["LP"][0]["quantity"], 2);
        assert!(v.get("market").is_none());
        let back: Card = serde_json::from_value(v).unwrap();
        assert_eq!(back, c);
    }
}
