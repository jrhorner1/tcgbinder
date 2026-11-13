//! What a trading card game provides to the app. Each game lives in its own crate (Magic
//! in `tcgbinder-mtg`) and keeps its own data folder, so games never share files.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use anyhow::Result;
use serde::Serialize;

use crate::collection::{Card, CatalogUpdate, SetInfo};
use crate::scan::Reading;

/// Progress callbacks receive the work done so far and the total.
pub type Progress<'a> = &'a mut dyn FnMut(usize, usize);

/// A trading card game the app can manage.
pub trait Game: Send + Sync {
    /// A short stable identifier, used for the game's data folder (e.g. "mtg").
    fn id(&self) -> &'static str;
    /// The display name (e.g. "Magic: The Gathering").
    fn name(&self) -> &'static str;

    /// Prepares the game to use `dir`, its private data folder, for caches.
    fn open(&self, dir: &Path) -> Result<()>;

    // Catalog lookups. Returned cards carry catalog data only; ownership (copies) is
    // left for the caller.

    /// One page of catalog search results for a name and/or set; `next_page` is the
    /// previous page's token.
    fn search(&self, text: &str, set: &str, next_page: &str) -> Result<SearchPage>;
    /// Placeholder text for the Add page's name and set fields.
    fn search_hints(&self) -> (&'static str, &'static str);
    /// Fresh prices (and rules text) for the printings with these IDs.
    fn refresh(&self, ids: &[String], progress: Progress) -> Result<HashMap<String, CatalogUpdate>>;
    /// Every set, keyed by uppercase set code.
    fn sets(&self) -> Result<HashMap<String, SetInfo>>;

    /// Reads another app's export (CSV) and matches it to catalog printings.
    fn import(&self, data: &[u8], progress: Progress) -> Result<(Vec<Card>, ImportReport)>;

    /// Every printing of the same card (e.g. for moving a card to a different set),
    /// newest first.
    fn printings(&self, c: &Card) -> Result<Vec<Card>>;
    /// Every card in a set (by uppercase code), in collector-number order.
    fn set_cards(&self, code: &str) -> Result<Vec<Card>>;

    // Presentation.

    /// The finish IDs as stored, e.g. "nonfoil", the usual one first.
    fn finishes(&self) -> &'static [&'static str];
    /// A finish's display name, e.g. "Normal".
    fn finish_name(&self, finish: &str) -> String;
    /// The rarity filter options, lowest first.
    fn rarities(&self) -> &'static [&'static str];
    /// Extra filters shown with set, rarity and finish.
    fn filters(&self) -> Vec<Filter>;
    /// Extra rows in the card editor.
    fn details(&self, c: &Card) -> Vec<Row>;
    /// A link to the card on the catalog's website: (label, URL).
    fn card_link(&self, c: &Card) -> Option<(String, String)>;

    /// The game's price history source, if it has one (Tools → Load price history).
    fn price_history(&self) -> Option<&dyn PriceHistorySource> {
        None
    }
    /// The game's card scanner, if it has one.
    fn scanner(&self) -> Option<&dyn Scanner> {
        None
    }
}

/// Daily prices by printing: catalog ID → finish → date → USD.
pub type DailyPriceMap = HashMap<String, HashMap<String, HashMap<String, f64>>>;

/// Implemented by games that can load daily price history.
pub trait PriceHistorySource: Send + Sync {
    /// Daily prices for the cards, reporting what it's doing through `status`.
    fn load(&self, cards: &[Card], status: &mut dyn FnMut(&str)) -> Result<DailyPriceMap>;
    /// Describes the source for the user, e.g. its size and window.
    fn info(&self) -> &'static str;
}

/// Implemented by games that can recognize a card from a photo: the text read off the
/// card (its name, set code, collector number, ...) is looked up in the catalog.
pub trait Scanner: Send + Sync {
    /// Summarizes what a reading says about the card, e.g. "shock|M19|156", and whether
    /// it says enough to look the card up. The camera captures once two readings in a
    /// row give the same complete key, so a blurry frame isn't used.
    fn scan_key(&self, r: &Reading) -> (String, bool);
    /// Looks up the card a reading describes.
    fn identify(&self, r: &Reading) -> Result<ScanResult>;
}

/// The outcome of identifying a scanned card.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScanResult {
    /// What was read off the card, e.g. "Shock · M19 · 156".
    pub read: String,
    /// The card name read (or matched), for searching by hand.
    pub name: String,
    /// The printings it could be, best first. Empty if nothing matched.
    pub candidates: Vec<Card>,
    /// The first candidate is certainly the printing scanned (its set and collector
    /// number were read and agree with the name).
    pub exact: bool,
    /// Explains a partial match, e.g. that the set couldn't be read.
    pub note: String,
}

/// One page of catalog search results.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SearchPage {
    pub cards: Vec<Card>,
    pub total: usize,
    /// An opaque token for the next page; "" on the last.
    pub next_page: String,
}

/// A summary of an import for the user.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImportReport {
    /// The detected source app, e.g. "EchoMTG".
    pub format: String,
    /// The rows read from the file.
    pub rows: usize,
    /// Rows needing attention (approximate matches, not found).
    pub notes: Vec<String>,
}

/// A game-specific filter menu, e.g. Magic's colors.
pub struct Filter {
    /// Shown as the menu's placeholder.
    pub name: &'static str,
    /// The choices; the app adds "Any".
    pub options: &'static [&'static str],
    pub matches: fn(&Card, &str) -> bool,
}

/// A label/value line in the card editor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Row {
    pub label: String,
    pub value: String,
}

/// Display names for a game's finishes.
pub fn finish_names(g: &dyn Game) -> BTreeMap<String, String> {
    g.finishes().iter().map(|f| (f.to_string(), g.finish_name(f))).collect()
}
