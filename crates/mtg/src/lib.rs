//! Magic: The Gathering for TCG Binder: Scryfall for the card catalog and prices, CSV
//! import from other collection apps, MTGJSON for daily price history, and card
//! scanning from the text on a card.

mod importer;
pub mod mtgjson;
mod scan;
pub mod scryfall;
#[cfg(any(test, feature = "testing"))]
pub mod testing;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result, bail};
use serde_json::Value;

use tcgbinder_core::collection::{Card, CatalogUpdate, Purchase, SetInfo};
use tcgbinder_core::dates;
use tcgbinder_core::game::{self, DailyPriceMap, Filter, Game, ImportReport, PriceHistorySource, Progress, Row, SearchPage};

/// The game's identifier and data folder name.
pub const ID: &str = "mtg";

/// The languages Magic cards are printed in, by Scryfall's code (the codes
/// `importer::parse_language` returns).
pub const LANGUAGES: &[(&str, &str)] = &[
    ("en", "English"),
    ("es", "Spanish"),
    ("fr", "French"),
    ("de", "German"),
    ("it", "Italian"),
    ("pt", "Portuguese"),
    ("ja", "Japanese"),
    ("ko", "Korean"),
    ("ru", "Russian"),
    ("zhs", "Simplified Chinese"),
    ("zht", "Traditional Chinese"),
    ("he", "Hebrew"),
    ("la", "Latin"),
    ("grc", "Ancient Greek"),
    ("ar", "Arabic"),
    ("sa", "Sanskrit"),
    ("ph", "Phyrexian"),
];

/// Magic, using Scryfall and MTGJSON.
pub struct Magic {
    pub scryfall: scryfall::Client,
    pub mtgjson: mtgjson::Client,
    dir: Mutex<PathBuf>,
    /// The cache for [`Game::set_cards`].
    set_cards: Mutex<HashMap<String, Vec<Card>>>,
}

impl Default for Magic {
    /// Magic using the live Scryfall and MTGJSON services.
    fn default() -> Self {
        Magic::with_clients(scryfall::Client::default(), mtgjson::Client::default())
    }
}

impl Magic {
    pub fn with_clients(scryfall: scryfall::Client, mtgjson: mtgjson::Client) -> Magic {
        Magic { scryfall, mtgjson, dir: Mutex::default(), set_cards: Mutex::default() }
    }
}

/// Copies a printing's catalog data into a collection card. Magic-only fields go into
/// its attrs.
pub fn from_scryfall(sc: &scryfall::Card) -> Card {
    let face = sc.card_faces.first();
    let mana_cost = if sc.mana_cost.is_empty() { face.map(|f| f.mana_cost.clone()).unwrap_or_default() } else { sc.mana_cost.clone() };
    let mut colors = sc.colors.clone();
    if colors.is_empty() {
        // Double-faced cards only carry colors per face.
        for col in sc.card_faces.iter().flat_map(|f| &f.colors) {
            if !colors.contains(col) {
                colors.push(col.clone());
            }
        }
    }
    let mut c = Card {
        id: sc.id.clone(),
        name: sc.name.clone(),
        set: sc.set.to_uppercase(),
        set_name: sc.set_name.clone(),
        collector_number: sc.collector_number.clone(),
        rarity: sc.rarity.clone(),
        released_at: sc.released_at.clone(),
        image_uri: sc.image_url().to_string(),
        type_line: sc.type_line.clone(),
        rules_text: sc.text(),
        language: sc.lang.clone(),
        finishes: sc.finishes.clone(),
        market: market_from_prices(&sc.prices, &sc.finishes),
        prices_updated: dates::today_string(),
        ..Card::default()
    };
    let text = |s: &str| if s.is_empty() { Value::Null } else { Value::from(s) };
    let num = |n: i64| if n == 0 { Value::Null } else { Value::from(n) };
    c.set_attr("oracle_id", text(&sc.oracle_id));
    c.set_attr("multiverse_ids", if sc.multiverse_ids.is_empty() { Value::Null } else { Value::from(sc.multiverse_ids.clone()) });
    c.set_attr("mtgo_id", num(sc.mtgo_id));
    c.set_attr("arena_id", num(sc.arena_id));
    c.set_attr("tcgplayer_id", num(sc.tcgplayer_id));
    c.set_attr("cardmarket_id", num(sc.cardmarket_id));
    c.set_attr("mana_cost", text(&mana_cost));
    c.set_attr("cmc", sc.cmc);
    c.set_attr("colors", colors);
    c.set_attr("color_identity", sc.color_identity.clone());
    c.set_attr("set_id", text(&sc.set_id));
    c
}

/// Scryfall's USD prices as a price per finish. Printings that come in etched but have
/// no etched price use the foil price.
fn market_from_prices(p: &scryfall::Prices, finishes: &[String]) -> BTreeMap<String, f64> {
    let parse = |raw: &Option<String>| raw.as_deref().and_then(|s| s.parse::<f64>().ok());
    let mut etched = parse(&p.usd_etched);
    if etched.is_none() && finishes.iter().any(|f| f == "etched") {
        etched = parse(&p.usd_foil);
    }
    [("nonfoil", parse(&p.usd)), ("foil", parse(&p.usd_foil)), ("etched", etched)]
        .into_iter()
        .filter_map(|(f, v)| v.map(|v| (f.to_string(), v)))
        .collect()
}

impl Magic {
    /// Scryfall prices no non-English printing, so each one in `foreign` takes the
    /// prices of the English printing with the same set and collector number, marked
    /// with `price_from = "en"`. Printings with no English counterpart stay unpriced.
    fn borrow_english_prices(&self, foreign: &[scryfall::Card], out: &mut HashMap<String, CatalogUpdate>) -> Result<()> {
        let key = |set: &str, number: &str| (set.to_lowercase(), number.to_lowercase());
        let mut wanted: Vec<(String, String)> = foreign.iter().map(|sc| key(&sc.set, &sc.collector_number)).collect();
        wanted.sort();
        wanted.dedup();
        let mut english: HashMap<(String, String), scryfall::Card> = HashMap::new();
        for chunk in wanted.chunks(scryfall::COLLECTION_BATCH_SIZE) {
            let bodies: Vec<_> = chunk
                .iter()
                .map(|(set, number)| {
                    serde_json::Map::from_iter([
                        ("set".to_string(), Value::from(set.as_str())),
                        ("collector_number".into(), Value::from(number.as_str())),
                    ])
                })
                .collect();
            let resp = self.scryfall.fetch_collection(&bodies).context("looking up English prices failed")?;
            for en in resp.data.into_iter().filter(|c| c.lang == "en") {
                english.insert(key(&en.set, &en.collector_number), en);
            }
        }
        for sc in foreign {
            let Some(en) = english.get(&key(&sc.set, &sc.collector_number)) else { continue };
            let market = market_from_prices(&en.prices, &sc.finishes);
            if let (false, Some(update)) = (market.is_empty(), out.get_mut(&sc.id)) {
                update.market = market;
                update.price_from = "en".into();
            }
        }
        Ok(())
    }
}

impl Game for Magic {
    fn id(&self) -> &'static str {
        ID
    }

    fn name(&self) -> &'static str {
        "Magic: The Gathering"
    }

    fn open(&self, dir: &Path) -> Result<()> {
        *self.dir.lock().unwrap() = dir.to_path_buf();
        Ok(())
    }

    /// Searches card names as a phrase (newest printings first); when that finds
    /// nothing, searches again with the words anywhere in a name.
    fn search(&self, text: &str, set: &str, next_page: &str) -> Result<SearchPage> {
        let q = scryfall::build_search_query(text, set)?;
        let mut page = self.scryfall.search_cards(&q, next_page)?;
        if next_page.is_empty() && page.cards.is_empty() {
            let plain = scryfall::build_plain_query(text, set)?;
            if plain != q {
                page = self.scryfall.search_cards(&plain, "")?;
            }
        }
        Ok(SearchPage { cards: page.cards.iter().map(from_scryfall).collect(), total: page.total, next_page: page.next_page })
    }

    /// Scryfall's name suggestions, from 2 characters typed.
    fn autocomplete(&self, text: &str) -> Result<Vec<String>> {
        let text = text.trim();
        if text.chars().count() < 2 {
            return Ok(Vec::new());
        }
        self.scryfall.autocomplete(text)
    }

    fn search_hints(&self) -> (&'static str, &'static str) {
        ("Card name, e.g. Scapeshift", "Set, e.g. M19")
    }

    fn refresh(&self, ids: &[String], progress: Progress) -> Result<HashMap<String, CatalogUpdate>> {
        let list: Vec<&String> = ids.iter().filter(|id| scryfall::is_uuid(id)).collect();
        let mut out = HashMap::new();
        let mut foreign = Vec::new(); // non-English printings without prices
        for (n, chunk) in list.chunks(scryfall::COLLECTION_BATCH_SIZE).enumerate() {
            let bodies: Vec<_> =
                chunk.iter().map(|id| serde_json::Map::from_iter([("id".to_string(), Value::from(id.as_str()))])).collect();
            let resp = self.scryfall.fetch_collection(&bodies).context("price refresh failed")?;
            for sc in resp.data {
                let update = CatalogUpdate {
                    market: market_from_prices(&sc.prices, &sc.finishes),
                    type_line: sc.type_line.clone(),
                    rules_text: sc.text(),
                    price_from: String::new(),
                };
                if update.market.is_empty() && !sc.lang.is_empty() && sc.lang != "en" {
                    foreign.push(sc.clone());
                }
                out.insert(sc.id, update);
            }
            progress(((n + 1) * scryfall::COLLECTION_BATCH_SIZE).min(list.len()), list.len());
        }
        self.borrow_english_prices(&foreign, &mut out)?;
        Ok(out)
    }

    /// Scryfall's set list, cached for a week in the game's data folder.
    fn sets(&self) -> Result<HashMap<String, SetInfo>> {
        let path = self.dir.lock().unwrap().join("scryfall-sets.json");
        scryfall::load_sets(&self.scryfall, &path)
    }

    /// Reads a CSV export, matches every row to its exact Scryfall printing and returns
    /// the cards to add.
    fn import(&self, data: &[u8], progress: Progress) -> Result<(Vec<Card>, ImportReport)> {
        let parsed = importer::parse_any(data)?;
        let format = parsed.format;
        let mut report = ImportReport {
            format: format.clone(),
            rows: parsed.rows.len(),
            notes: parsed.warnings,
            unmatched: Vec::new(),
            header: parsed.header,
        };
        if parsed.rows.is_empty() {
            bail!("no card rows found in this {format} file");
        }
        let (mut approx, mut missing, mut cards) = (Vec::new(), Vec::new(), Vec::new());
        for r in importer::resolve(&self.scryfall, parsed.rows, progress) {
            let Some(sc) = &r.card else {
                tracing::info!(row = r.row.describe(), reason = r.error, "import row not matched");
                missing.push(format!("  {}", r.row.describe()));
                // A row split into regular and foil copies is one line of the file.
                if report.unmatched.last().is_none_or(|u| u.line != r.row.line) {
                    report.unmatched.push(game::UnmatchedRow { line: r.row.line, text: r.row.text.clone() });
                }
                continue;
            };
            if r.approximate {
                approx.push(format!("  {} → {} · {}", r.row.describe(), sc.set.to_uppercase(), sc.collector_number));
            }
            let mut card = from_scryfall(sc);
            let purchase = Purchase {
                quantity: r.row.quantity,
                price_paid: r.row.price_paid,
                acquired_date: r.row.acquired_date.clone(),
                note: r.row.note.clone(),
                ..Default::default()
            };
            card.add_purchase(&importer::choose_finish(&r.row.finish, &sc.finishes), r.row.condition, purchase);
            cards.push(card);
        }
        if !approx.is_empty() {
            report.notes.push(format!("Matched by name only, so the printing may differ ({}) — please check:", approx.len()));
            report.notes.extend(approx);
        }
        if !missing.is_empty() {
            report.notes.push(format!("Not imported — no match on Scryfall ({}):", missing.len()));
            report.notes.extend(missing);
        }
        Ok((cards, report))
    }

    /// Every printing of the card, found by its Oracle ID (or exact name), in a
    /// language (Scryfall's `lang:`).
    fn printings(&self, c: &Card, language: &str) -> Result<Vec<Card>> {
        let oracle = c.attr_str("oracle_id");
        let mut q = if oracle.is_empty() { format!("!\"{}\"", importer::front_face(&c.name)) } else { format!("oracleid:{oracle}") };
        if LANGUAGES.iter().any(|(code, _)| *code == language) {
            q = format!("{q} lang:{language}");
        }
        let mut out: Vec<Card> = self.scryfall.search_all(&q, 5)?.iter().map(from_scryfall).collect();
        out.sort_by(|a, b| b.released_at.cmp(&a.released_at));
        Ok(out)
    }

    /// A set's cards, cached for the session (sets rarely change).
    fn set_cards(&self, code: &str) -> Result<Vec<Card>> {
        let code = code.to_uppercase();
        if let Some(cached) = self.set_cards.lock().unwrap().get(&code) {
            return Ok(cached.clone());
        }
        let found: Vec<Card> = self.scryfall.search_all(&format!("e:{}", code.to_lowercase()), 20)?.iter().map(from_scryfall).collect();
        self.set_cards.lock().unwrap().insert(code, found.clone());
        Ok(found)
    }

    fn finishes(&self) -> &'static [&'static str] {
        &["nonfoil", "foil", "etched"]
    }

    /// The finish as players say it: Scryfall's "nonfoil" is "Normal".
    fn finish_name(&self, f: &str) -> String {
        match f {
            "nonfoil" => "Normal".into(),
            "" => String::new(),
            other => {
                let mut c = other.chars();
                c.next().map(|first| first.to_uppercase().chain(c).collect()).unwrap_or_default()
            }
        }
    }

    fn languages(&self) -> &'static [(&'static str, &'static str)] {
        LANGUAGES
    }

    fn is_basic_land(&self, c: &Card) -> bool {
        c.type_line.contains("Basic Land")
    }

    fn rarities(&self) -> &'static [&'static str] {
        &["common", "uncommon", "rare", "mythic", "special", "bonus"]
    }

    fn filters(&self) -> Vec<Filter> {
        const COLORS: &[&str] = &["W", "U", "B", "R", "G", COLOR_MULTICOLOR, COLOR_COLORLESS];
        vec![
            Filter { name: "Color", options: COLORS, matches: matches_color },
            Filter { name: "Color identity", options: COLORS, matches: matches_color_identity },
            Filter { name: "Mana value", options: &["0", "1", "2", "3", "4", "5", "6", "7+"], matches: matches_mana_value },
        ]
    }

    fn details(&self, c: &Card) -> Vec<Row> {
        let mana = c.attr_str("mana_cost");
        if mana.is_empty() { Vec::new() } else { vec![Row { label: "Mana cost".into(), value: mana.into() }] }
    }

    fn catalog_id_label(&self) -> &'static str {
        "Scryfall ID"
    }

    fn card_link(&self, c: &Card) -> Option<(String, String)> {
        let seg = scryfall::escape;
        Some(("View on Scryfall".into(), format!("https://scryfall.com/card/{}/{}", seg(&c.set.to_lowercase()), seg(&c.collector_number))))
    }

    fn price_history(&self) -> Option<&dyn PriceHistorySource> {
        Some(self)
    }

    fn scanner(&self) -> Option<&dyn game::Scanner> {
        Some(self)
    }
}

const COLOR_MULTICOLOR: &str = "Multicolor";
const COLOR_COLORLESS: &str = "Colorless";

/// A single color also matches the multicolor cards that include it.
fn matches_color(c: &Card, want: &str) -> bool {
    matches_colors(&c.attr_strings("colors"), want)
}

/// Color identity, as Commander uses it: the same choices as Color.
fn matches_color_identity(c: &Card, want: &str) -> bool {
    matches_colors(&c.attr_strings("color_identity"), want)
}

fn matches_colors(colors: &[&str], want: &str) -> bool {
    match want {
        COLOR_COLORLESS => colors.is_empty(),
        COLOR_MULTICOLOR => colors.len() > 1,
        _ => colors.iter().any(|col| col.eq_ignore_ascii_case(want)),
    }
}

/// Mana value: "0" to "6" exactly, "7+" for seven or more.
fn matches_mana_value(c: &Card, want: &str) -> bool {
    let Some(mv) = c.attrs.get("cmc").and_then(Value::as_f64) else { return false };
    match want.strip_suffix('+') {
        Some(min) => min.parse::<f64>().is_ok_and(|min| mv >= min),
        None => want.parse::<f64>().is_ok_and(|n| mv == n),
    }
}

impl PriceHistorySource for Magic {
    fn info(&self) -> &'static str {
        "the last 90 days of TCGplayer prices from MTGJSON (about a 50 MB download)"
    }

    /// Matches the cards' printings to MTGJSON through the per-set files and returns
    /// MTGJSON's daily prices for them, keyed by Scryfall ID.
    fn load(&self, cards: &[Card], status: &mut dyn FnMut(&str)) -> Result<DailyPriceMap> {
        let mut set_codes: Vec<String> = Vec::new();
        for c in cards {
            let code = c.set.to_uppercase();
            if !code.is_empty() && !set_codes.contains(&code) {
                set_codes.push(code);
            }
        }
        let ids: HashSet<&str> = cards.iter().map(|c| c.id.as_str()).collect();
        status(&format!("Matching {} sets with MTGJSON…", set_codes.len()));
        let uuids = self.mtgjson.uuids(&set_codes)?;
        let scryfall_by_uuid: HashMap<String, String> =
            ids.iter().filter_map(|id| uuids.get(*id).map(|u| (u.clone(), id.to_string()))).collect();
        if scryfall_by_uuid.is_empty() {
            bail!("none of your cards were found on MTGJSON");
        }
        status("Downloading price history…");
        let want: HashSet<String> = scryfall_by_uuid.keys().cloned().collect();
        let prices = self.mtgjson.price_history(&want, &mut |read, total| {
            if total > 0 {
                status(&format!("Downloading price history… {} of {} MB", read >> 20, total >> 20));
            }
        })?;
        Ok(prices.into_iter().filter_map(|(u, dp)| scryfall_by_uuid.get(&u).map(|id| (id.clone(), dp))).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_scryfall_keeps_magic_fields_as_attrs() {
        let resp: scryfall::SearchResponse = serde_json::from_str(include_str!("../tests/search-scapeshift.json")).unwrap();
        let c = from_scryfall(&resp.data[0]);
        assert_eq!((c.name.as_str(), c.set.as_str(), c.collector_number.as_str()), ("Scapeshift", "M19", "201"));
        assert!(!c.rules_text.is_empty());
        assert_eq!(c.attr_str("mana_cost"), "{2}{G}{G}");
        assert_eq!(c.attr_strings("colors"), ["G"]);
        assert!(!c.attr_str("oracle_id").is_empty());
        assert!(c.market["nonfoil"] > 0.0 && c.market["foil"] > 0.0);
    }

    #[test]
    fn etched_falls_back_to_foil() {
        let p = scryfall::Prices { usd: Some("1.00".into()), usd_foil: Some("2.50".into()), usd_etched: None };
        let m = market_from_prices(&p, &["nonfoil".into(), "etched".into()]);
        assert_eq!(m["etched"], 2.5);
        assert!(market_from_prices(&scryfall::Prices::default(), &[]).is_empty());
    }

    #[test]
    fn color_filter() {
        let mk = |colors: &[&str]| {
            let mut c = Card::default();
            c.set_attr("colors", colors.to_vec());
            c
        };
        for (colors, opt, want) in [
            (&["R"][..], "R", true),
            (&["U", "R"], "R", true),
            (&["G"], "R", false),
            (&[], "Colorless", true),
            (&["G"], "Colorless", false),
            (&["U", "R"], "Multicolor", true),
            (&["R"], "Multicolor", false),
        ] {
            assert_eq!(matches_color(&mk(colors), opt), want, "{colors:?} {opt}");
        }
        assert_eq!(Magic::default().finish_name("nonfoil"), "Normal");
        assert_eq!(Magic::default().finish_name("etched"), "Etched");
    }

    #[test]
    fn identity_and_mana_value_filters() {
        let mut c = Card::default();
        c.set_attr("colors", Vec::<String>::new());
        c.set_attr("color_identity", vec!["G", "U"]);
        c.set_attr("cmc", 7.0);
        assert!(!matches_color(&c, "G") && matches_color(&c, COLOR_COLORLESS), "an artifact with a green-blue ability");
        assert!(matches_color_identity(&c, "G") && matches_color_identity(&c, COLOR_MULTICOLOR));
        assert!(!matches_color_identity(&c, COLOR_COLORLESS));
        assert!(matches_mana_value(&c, "7+") && matches_mana_value(&c, "7") && !matches_mana_value(&c, "6"));
        c.set_attr("cmc", 3.0);
        assert!(matches_mana_value(&c, "3") && !matches_mana_value(&c, "7+"));
        assert!(!matches_mana_value(&Card::default(), "0"), "no mana value: matches nothing");
        let names: Vec<&str> = Magic::default().filters().iter().map(|f| f.name).collect();
        assert_eq!(names, ["Color", "Color identity", "Mana value"]);
    }

    /// A fake Scryfall with Sol Ring in Commander 2021 (263) in English ($1.85) and
    /// Japanese (no prices), a Japanese-only promo, and an English card of its own.
    fn language_server() -> testing::FakeServer {
        use scryfall::{CollectionResponse, Prices};
        let card = |id: &str, lang: &str, set: &str, number: &str, usd: Option<&str>| scryfall::Card {
            id: id.into(),
            name: "Sol Ring".into(),
            lang: lang.into(),
            set: set.into(),
            collector_number: number.into(),
            finishes: vec!["nonfoil".into(), "foil".into()],
            prices: Prices { usd: usd.map(String::from), ..Default::default() },
            ..Default::default()
        };
        let catalog = [
            card(JA, "ja", "c21", "263", None),
            card(EN, "en", "c21", "263", Some("1.85")),
            card(PROMO, "ja", "pjp", "1", None),
            card(OTHER, "en", "m19", "1", Some("0.50")),
        ];
        testing::FakeServer::start(move |req| {
            if let Some(rest) = req.path.strip_prefix("/cards/") {
                // /cards/{set}/{number}/{lang}: one printing in a language.
                let parts: Vec<&str> = rest.split('/').collect();
                if let [set, number, lang] = parts[..] {
                    return match catalog.iter().find(|c| c.set == set && c.collector_number == number && c.lang == lang) {
                        Some(c) => testing::Reply::json(c),
                        None => testing::Reply::not_found(),
                    };
                }
            }
            let body: Value = serde_json::from_slice(&req.body).unwrap_or_default();
            let mut resp = CollectionResponse::default();
            for id in body["identifiers"].as_array().cloned().unwrap_or_default() {
                // By set and number Scryfall answers with the English printing, if any.
                let hit = catalog.iter().find(|c| match id.get("id") {
                    Some(want) => c.id == *want,
                    None => c.set == id["set"] && c.collector_number == id["collector_number"] && (c.lang == "en" || c.set == "pjp"),
                });
                match hit {
                    Some(c) => resp.data.push(c.clone()),
                    None => resp.not_found.push(id.as_object().cloned().unwrap_or_default()),
                }
            }
            testing::Reply::json(&resp)
        })
    }

    const JA: &str = "bbbbbbbb-0000-4000-8000-000000000001";
    const EN: &str = "bbbbbbbb-0000-4000-8000-000000000002";
    const PROMO: &str = "bbbbbbbb-0000-4000-8000-000000000003";
    const OTHER: &str = "bbbbbbbb-0000-4000-8000-000000000004";

    #[test]
    fn foreign_printings_take_the_english_price() {
        let srv = language_server();
        let g = Magic::with_clients(scryfall::Client::unlimited(&srv.url), mtgjson::Client::new(&srv.url));
        let ids: Vec<String> = [JA, PROMO, OTHER].iter().map(|s| s.to_string()).collect();
        let got = g.refresh(&ids, &mut |_, _| {}).unwrap();
        assert_eq!(got[JA].market, BTreeMap::from([("nonfoil".to_string(), 1.85)]));
        assert_eq!(got[JA].price_from, "en");
        assert!(got[PROMO].market.is_empty() && got[PROMO].price_from.is_empty(), "no English printing: unpriced");
        assert_eq!((got[OTHER].market["nonfoil"], got[OTHER].price_from.as_str()), (0.5, ""));
        let calls = srv.calls.lock().unwrap().len();
        assert_eq!(calls, 2, "one batch by ID, one for the English printings");
    }

    #[test]
    fn imported_foreign_cards_are_priced_at_once() {
        let srv = language_server();
        let g = Magic::with_clients(scryfall::Client::unlimited(&srv.url), mtgjson::Client::new(&srv.url));
        let dir = tempfile::tempdir().unwrap();
        let svc = tcgbinder_core::service::GameService::open(dir.path(), dir.path(), Box::new(g)).unwrap();
        let res = svc.import(b"Name,Set Code,Collector Number,Language,Quantity\nSol Ring,C21,263,Japanese,2\n", &mut |_, _| {}).unwrap();
        assert_eq!(res.cards, 2);
        let card = svc.store.get(JA).expect("the Japanese printing");
        assert_eq!((card.market_price("nonfoil"), card.price_from.as_str()), (Some(1.85), "en"));
    }

    #[test]
    fn phrase_search_falls_back_to_words() {
        // A Scryfall where only the loose word search finds anything.
        let srv = testing::FakeServer::start(|req| match (req.path.as_str(), req.query.get("q").map(String::as_str)) {
            ("/cards/search", Some(q)) if !q.starts_with("name:") => testing::Reply::json(&scryfall::SearchResponse {
                total_cards: 1,
                data: vec![scryfall::Card { id: "x".into(), name: "Sol Ring".into(), ..Default::default() }],
                ..Default::default()
            }),
            ("/cards/autocomplete", Some(q)) => testing::Reply::json(&serde_json::json!({ "data": [format!("{q} Ring")] })),
            _ => testing::Reply::not_found(),
        });
        let g = Magic::with_clients(scryfall::Client::unlimited(&srv.url), mtgjson::Client::new(&srv.url));
        let page = g.search("sol rin", "", "").unwrap();
        assert_eq!(page.cards.len(), 1);
        let calls = srv.calls.lock().unwrap().clone();
        assert_eq!(calls.len(), 2, "{calls:?}");
        assert!(calls[0].contains("q=name%3A%22sol+rin%22") && calls[0].contains("order=released") && calls[0].contains("dir=desc"));
        assert!(calls[1].contains("q=sol+rin&"));

        assert_eq!(g.autocomplete("Sol").unwrap(), ["Sol Ring"]);
        assert!(g.autocomplete("S").unwrap().is_empty(), "two characters at least");
    }

    /// Imports a real export against live Scryfall into a temporary collection; skipped
    /// unless MTG_IMPORT_FILE points at a CSV.
    #[test]
    fn live_import() {
        let Ok(path) = std::env::var("MTG_IMPORT_FILE") else { return };
        let data = std::fs::read(path).unwrap();
        let g = Magic::default();
        let (cards, report) = g.import(&data, &mut |_, _| {}).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = tcgbinder_core::collection::Store::open(dir.path().join("collection.json"), ID).unwrap();
        let copies: i64 = cards.iter().map(Card::copy_count).sum();
        let (created, merged) = store.add_cards(cards).unwrap();
        eprintln!(
            "{}: {} rows, {copies} cards, {created} new cards, {merged} merged, {} notes",
            report.format,
            report.rows,
            report.notes.len()
        );
        // Importing again only adds to quantities.
        let (again, _) = g.import(&data, &mut |_, _| {}).unwrap();
        assert_eq!(store.add_cards(again).unwrap().0, 0);
    }
}
