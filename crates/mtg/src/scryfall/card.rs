use serde::{Deserialize, Serialize};

/// Scryfall's prices object (decimal strings; missing when unknown).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prices {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usd_foil: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usd_etched: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImageUris {
    pub normal: String,
}

/// One face of a multi-faced card.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Face {
    pub name: String,
    pub mana_cost: String,
    pub oracle_text: String,
    pub colors: Vec<String>,
    pub image_uris: ImageUris,
}

/// The part of a Scryfall card object the app uses.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Card {
    pub id: String,
    pub oracle_id: String,
    pub multiverse_ids: Vec<i64>,
    pub mtgo_id: i64,
    pub arena_id: i64,
    pub tcgplayer_id: i64,
    pub cardmarket_id: i64,
    pub name: String,
    pub lang: String,
    pub released_at: String,
    pub oracle_text: String,
    pub type_line: String,
    pub cmc: f64,
    pub image_uris: ImageUris,
    pub card_faces: Vec<Face>,
    pub mana_cost: String,
    pub colors: Vec<String>,
    pub color_identity: Vec<String>,
    pub finishes: Vec<String>,
    pub set_id: String,
    pub set: String,
    pub set_name: String,
    pub collector_number: String,
    pub rarity: String,
    pub prices: Prices,
}

impl Card {
    /// The card's image, falling back to the front face for double-faced cards.
    pub fn image_url(&self) -> &str {
        if !self.image_uris.normal.is_empty() {
            return &self.image_uris.normal;
        }
        self.card_faces.iter().map(|f| f.image_uris.normal.as_str()).find(|u| !u.is_empty()).unwrap_or("")
    }

    /// The rules text, joining the faces of multi-faced cards.
    pub fn text(&self) -> String {
        if !self.oracle_text.is_empty() || self.card_faces.is_empty() {
            return self.oracle_text.clone();
        }
        self.card_faces.iter().map(|f| format!("{}\n{}", f.name, f.oracle_text)).collect::<Vec<_>>().join("\n\n")
    }
}

#[cfg(test)]
mod tests {
    use super::super::SearchResponse;

    /// The response and card types against a real Scryfall /cards/search answer.
    #[test]
    fn decode_search_response() {
        let resp: SearchResponse = serde_json::from_str(include_str!("../../tests/search-scapeshift.json")).unwrap();
        assert_eq!((resp.total_cards, resp.data.len(), resp.has_more), (1, 1, false));
        let c = &resp.data[0];
        assert_eq!((c.name.as_str(), c.set.as_str(), c.collector_number.as_str()), ("Scapeshift", "m19", "201"));
        assert!(super::super::is_uuid(&c.id));
        assert!(c.prices.usd.is_some() && !c.image_url().is_empty() && !c.text().is_empty() && !c.finishes.is_empty());
    }
}
