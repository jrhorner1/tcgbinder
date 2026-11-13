use std::collections::{HashMap, HashSet};

use anyhow::{Result, bail};
use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime};

use tcgbinder_core::Condition;

use super::{parse_finish, parse_language};

/// One line of an external collection export, normalized but not yet matched to a
/// Scryfall printing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    /// The 1-based line in the source file, for reports.
    pub line: u64,
    pub name: String,
    /// A lowercase Scryfall-style set code, when known.
    pub set_code: String,
    pub set_name: String,
    /// The collector number.
    pub number: String,
    pub scryfall_id: String,
    pub multiverse_id: i64,
    pub mtgo_id: i64,
    pub tcgplayer_id: i64,
    pub quantity: i64,
    /// "nonfoil", "foil" or "etched".
    pub finish: String,
    pub condition: Condition,
    /// Scryfall's language code, e.g. "en", "ja".
    pub language: String,
    pub price_paid: f64,
    /// `YYYY-MM-DD`, or "".
    pub acquired_date: String,
    pub note: String,
    /// The row as it appears in the file, so unmatched rows can be handed back.
    pub text: String,
}

impl Row {
    /// A short label for reports, e.g. "Line 12: Lightning Bolt (2XM · 117)".
    pub fn describe(&self) -> String {
        let mut place = self.set_code.to_uppercase();
        if place.is_empty() {
            place = self.set_name.clone();
        }
        if !self.number.is_empty() {
            place = format!("{place} · {}", self.number);
        }
        let name = if self.name.is_empty() { "(no name)" } else { &self.name };
        if place.is_empty() { format!("Line {}: {name}", self.line) } else { format!("Line {}: {name} ({place})", self.line) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Field {
    Name,
    SetCode,
    SetName,
    /// A "Set" or "Edition" column that may hold either a code or a full name.
    SetAmbiguous,
    Number,
    ScryfallId,
    MultiverseId,
    MtgoId,
    TcgplayerId,
    Quantity,
    /// Foil copies, in files with separate regular and foil quantity columns.
    QuantityFoil,
    Finish,
    Condition,
    Language,
    Price,
    Date,
    Note,
}

/// Normalized header names (lowercase, '_' and '-' as spaces) for each field. Matching
/// is exact, never by substring, so columns like "foil_price" or "price_updated" can't be
/// mistaken for the finish or purchase price.
const HEADER_ALIASES: &[(Field, &[&str])] = &[
    (Field::Name, &["name", "card name", "card", "card title"]),
    (Field::SetCode, &["set code", "edition code", "setcode", "set id"]),
    (Field::SetName, &["set name", "expansion", "edition name", "expansion name"]),
    (Field::SetAmbiguous, &["set", "edition"]),
    (Field::Number, &["collector number", "card number", "number", "collector no", "collector #", "cn", "card #", "collectors number"]),
    (Field::ScryfallId, &["scryfall id", "scryfallid", "scryfall uuid", "scryfall card id"]),
    (Field::MultiverseId, &["multiverse id", "multiverseid", "mid", "multiverse"]),
    (Field::MtgoId, &["mtgo id", "mtgoid"]),
    (Field::TcgplayerId, &["tcgplayer id", "tcgplayer product id", "product id", "tcg id", "tcgplayer productid"]),
    (Field::Quantity, &["quantity", "count", "qty", "amount", "total quantity", "reg qty", "regular qty", "non foil qty"]),
    (Field::QuantityFoil, &["foil qty", "foil quantity", "foil count"]),
    (Field::Finish, &["foil", "finish", "printing", "foil type", "is foil"]),
    (Field::Condition, &["condition", "cond"]),
    (Field::Language, &["language", "lang"]),
    (
        Field::Price,
        &["purchase price", "price acquired", "price bought", "price paid", "acquired price", "purchase cost", "buy price", "cost"],
    ),
    (Field::Date, &["date acquired", "date bought", "date added", "acquired date", "acquired", "purchase date", "acquired on", "added"]),
    (Field::Note, &["note", "notes", "comment", "comments"]),
];

fn normalize_header(h: &str) -> String {
    let h = h.trim_start_matches('\u{feff}').trim().to_lowercase().replace(['_', '-'], " ");
    h.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// An app's export format: how to recognise it, and how its columns differ from
/// [`HEADER_ALIASES`].
struct Profile {
    name: &'static str,
    detect: fn(&HashSet<String>) -> bool,
    /// Column meanings that differ from HEADER_ALIASES for this app.
    overrides: &'static [(Field, &'static [&'static str])],
    /// Each row is one copy, so a file without a quantity column is expected.
    rows_are_copies: bool,
}

impl Profile {
    const fn new(name: &'static str, detect: fn(&HashSet<String>) -> bool) -> Profile {
        Profile { name, detect, overrides: &[], rows_are_copies: false }
    }

    /// The field a normalized header names: this profile's overrides first, then the
    /// shared aliases.
    fn field(&self, header: &str) -> Option<Field> {
        self.overrides.iter().chain(HEADER_ALIASES).find(|(_, names)| names.contains(&header)).map(|(f, _)| *f)
    }
}

/// The known formats, checked in order; the last matches anything.
const PROFILES: &[Profile] = &[
    Profile {
        rows_are_copies: true,
        ..Profile::new("EchoMTG", |h| h.contains("emid") || h.contains("inventory id") && h.contains("price acquired"))
    },
    Profile::new("ManaBox", |h| h.contains("manabox id")),
    Profile::new("Dragon Shield", |h| h.contains("folder name") && h.contains("price bought")),
    Profile::new("Deckbox", |h| h.contains("tradelist count") && h.contains("edition code")),
    Profile::new("Moxfield", |h| h.contains("tradelist count")),
    Profile::new("TCGplayer", |h| h.contains("simple name") || h.contains("sku") && h.contains("product id")),
    Profile::new("Archidekt", |h| h.contains("scryfall id") && h.contains("date added")),
    Profile::new("Delver Lens", |h| h.contains("list name") && h.contains("edition")),
    Profile::new("Generic CSV", |_| true),
];

/// The format of a file with these normalized headers.
fn detect_profile(h: &HashSet<String>) -> &'static Profile {
    PROFILES.iter().find(|p| (p.detect)(h)).expect("the generic profile matches anything")
}

/// A collection export, read.
#[derive(Debug, Default)]
pub struct ParsedCsv {
    pub rows: Vec<Row>,
    /// The app it came from, e.g. "EchoMTG".
    pub format: String,
    /// Columns that couldn't be used, for the top of the import report.
    pub warnings: Vec<String>,
    /// The header line as it appears in the file.
    pub header: String,
}

/// Counts a column's non-empty values and the ones that couldn't be read.
#[derive(Default)]
struct Misses {
    values: usize,
    failed: usize,
}

impl Misses {
    fn note(&mut self, raw: &str, ok: bool) {
        if !raw.is_empty() {
            self.values += 1;
            self.failed += usize::from(!ok);
        }
    }

    /// A warning when more than half the non-empty values failed.
    fn warning(&self, what: &str, column: &str) -> Option<String> {
        (self.failed * 2 > self.values).then(|| {
            format!("The {what} column \"{column}\" couldn't be read in {} of {} rows; those were left blank.", self.failed, self.values)
        })
    }
}

/// Reads a collection export from any supported app: its rows, the app it came from,
/// and warnings about columns that couldn't be used.
pub fn parse_csv(data: &[u8]) -> Result<ParsedCsv> {
    let data = data.strip_prefix("\u{feff}".as_bytes()).unwrap_or(data);
    let mut r = csv::ReaderBuilder::new().has_headers(false).flexible(true).from_reader(data);
    // A malformed row is skipped rather than abandoning the import.
    let records: Vec<csv::StringRecord> = r.records().filter_map(|rec| rec.ok()).collect();
    // Each record's text in the file: from its start to the next record's.
    let starts: Vec<usize> = records.iter().map(|rec| rec.position().map_or(0, |p| p.byte() as usize)).collect();
    let text_of = |i: usize| {
        let end = starts.get(i + 1).copied().unwrap_or(data.len()).min(data.len());
        String::from_utf8_lossy(&data[starts[i].min(end)..end]).trim_end_matches(['\r', '\n']).to_string()
    };
    let mut first = 0;
    // Dragon Shield (and Excel) exports may start with a "sep=," hint line.
    if records.first().and_then(|h| h.get(0)).is_some_and(|h| h.trim().to_lowercase().starts_with("sep=")) {
        first = 1;
    }
    if records.len() <= first {
        bail!("could not read the header row: the file is empty");
    }
    let header_text = text_of(first);
    let headers = records[first].clone();
    let normalized: Vec<String> = headers.iter().map(normalize_header).collect();
    let profile = detect_profile(&normalized.iter().cloned().collect());

    let mut cols: HashMap<Field, usize> = HashMap::new();
    for (i, n) in normalized.iter().enumerate() {
        if let Some(field) = profile.field(n) {
            cols.entry(field).or_insert(i); // the first matching column wins
        }
    }
    let has = |f: Field| cols.contains_key(&f);
    if !has(Field::Name) && !has(Field::ScryfallId) && !has(Field::MultiverseId) && !has(Field::TcgplayerId) {
        bail!("unrecognized file: no card name or card ID column found in the header");
    }
    let column_name = |f: Field| cols.get(&f).and_then(|i| headers.get(*i)).unwrap_or("").trim().to_string();
    let split = has(Field::QuantityFoil);

    let (mut price_misses, mut date_misses) = (Misses::default(), Misses::default());
    let mut rows = Vec::new();
    for (i, rec) in records.iter().enumerate().skip(first + 1) {
        let get = |f: Field| cols.get(&f).and_then(|i| rec.get(*i)).map_or("", str::trim);
        let price = try_price(get(Field::Price));
        price_misses.note(get(Field::Price), price.is_some());
        let date = parse_date(get(Field::Date));
        date_misses.note(get(Field::Date), !date.is_empty());
        let mut row = Row {
            line: rec.position().map_or(0, |p| p.line()),
            name: get(Field::Name).to_string(),
            set_code: get(Field::SetCode).to_lowercase(),
            set_name: get(Field::SetName).to_string(),
            number: get(Field::Number).to_string(),
            scryfall_id: get(Field::ScryfallId).to_lowercase(),
            multiverse_id: parse_int_loose(get(Field::MultiverseId)),
            mtgo_id: parse_int_loose(get(Field::MtgoId)),
            tcgplayer_id: parse_int_loose(get(Field::TcgplayerId)),
            quantity: 1,
            finish: parse_finish(get(Field::Finish)).to_string(),
            condition: Condition::parse(get(Field::Condition)),
            language: parse_language(get(Field::Language)).to_string(),
            price_paid: price.unwrap_or(0.0),
            acquired_date: date,
            note: get(Field::Note).to_string(),
            text: text_of(i),
        };
        let amb = get(Field::SetAmbiguous);
        if !amb.is_empty() {
            // With a dedicated set-code column, "Set"/"Edition" holds the set name
            // (Deckbox, TCGplayer). Otherwise guess from its shape (Moxfield uses codes).
            if has(Field::SetCode) || !looks_like_set_code(amb) {
                if row.set_name.is_empty() && !amb.eq_ignore_ascii_case(&row.set_code) {
                    row.set_name = amb.to_string();
                }
            } else if row.set_code.is_empty() {
                row.set_code = amb.to_lowercase();
            }
        }
        if row.name.is_empty() && row.scryfall_id.is_empty() && row.multiverse_id == 0 && row.tcgplayer_id == 0 {
            continue;
        }
        if split {
            // Separate regular and foil quantities: one row for each, skipping zeros.
            let regular = parse_int_loose(get(Field::Quantity));
            let foil = parse_int_loose(get(Field::QuantityFoil));
            for (quantity, finish) in [(regular, "nonfoil"), (foil, "foil")] {
                if quantity > 0 {
                    rows.push(Row { quantity, finish: finish.into(), ..row.clone() });
                }
            }
            continue;
        }
        if has(Field::Quantity) {
            let q = parse_int_loose(get(Field::Quantity));
            if q <= 0 {
                continue; // explicit zero-count rows (e.g. wishlist entries) aren't owned cards
            }
            row.quantity = q;
        }
        rows.push(row);
    }

    let mut warnings = Vec::new();
    if !has(Field::Quantity) && !split && !profile.rows_are_copies {
        warnings.push("No quantity column was found, so every row was counted as 1 copy.".to_string());
    }
    warnings.extend(price_misses.warning("price", &column_name(Field::Price)));
    warnings.extend(date_misses.warning("date", &column_name(Field::Date)));
    Ok(ParsedCsv { rows, format: profile.name.to_string(), warnings, header: header_text })
}

fn looks_like_set_code(s: &str) -> bool {
    (2..=6).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// Accepts "3", "3.0" and " 3 ".
fn parse_int_loose(s: &str) -> i64 {
    let s = s.trim();
    s.parse().unwrap_or_else(|_| s.parse::<f64>().map_or(0, |f| f as i64))
}

#[cfg(test)]
fn parse_price(s: &str) -> f64 {
    try_price(s).unwrap_or(0.0)
}

/// A price in any common notation; `None` if it can't be read (or is negative).
fn try_price(s: &str) -> Option<f64> {
    let mut s = s.to_string();
    for sym in ["$", "€", "£", "USD", "EUR"] {
        s = s.replace(sym, "");
    }
    let s = s.trim();
    let s = if s.contains(',') && !s.contains('.') { s.replace(',', ".") } else { s.replace(',', "") }; // European decimal comma
    s.trim().parse::<f64>().ok().filter(|f| *f >= 0.0 && f.is_finite())
}

const DATE_FORMATS: &[&str] = &["%Y-%m-%d", "%Y/%m/%d", "%m/%d/%Y", "%m/%d/%y", "%b %d, %Y", "%d %b %Y"];
const DATETIME_FORMATS: &[&str] = &["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M:%S%.fZ"];

/// Converts common export date formats to `YYYY-MM-DD`; anything else gives "".
fn parse_date(s: &str) -> String {
    let s = s.trim();
    if s.is_empty() {
        return String::new();
    }
    let fmt = |d: NaiveDate| d.format("%Y-%m-%d").to_string();
    if let Some(d) = DATE_FORMATS.iter().find_map(|f| NaiveDate::parse_from_str(s, f).ok()) {
        // "%m/%d/%Y" also reads two-digit years as the first century; those are "%y".
        if d.year_ce().1 >= 100 || !s.contains('/') {
            return fmt(d);
        }
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%m/%d/%y") {
        return fmt(d);
    }
    if let Some(d) = DATETIME_FORMATS.iter().find_map(|f| NaiveDateTime::parse_from_str(s, f).ok()) {
        return fmt(d.date());
    }
    if let Ok(d) = DateTime::parse_from_rfc3339(s) {
        return fmt(d.date_naive());
    }
    // e.g. "2018-08-08 00:00:00 UTC"
    s.get(..10).and_then(|p| NaiveDate::parse_from_str(p, "%Y-%m-%d").ok()).map(fmt).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(r: Row) -> Row {
        Row { line: 0, text: String::new(), ..r }
    }

    #[test]
    fn formats() {
        let cases: &[(&str, &str, Row)] = &[
            (
                "EchoMTG",
                "inventory_id,user_id,emid,foil,condition,lang,price_acquired,date_acquired,name,set_code,expansion,collector_number,tcgplayer_id,mid,foil_price,price_change,price_updated\n\
                 1,2,3,1.0,LP,EN,0.11,2018-08-08,Forest (266),DOM,Dominaria,266,999,100055030,0.2,0.0,2026-10-31 15:28:08\n",
                Row {
                    name: "Forest (266)".into(),
                    set_code: "dom".into(),
                    set_name: "Dominaria".into(),
                    number: "266".into(),
                    tcgplayer_id: 999,
                    multiverse_id: 100055030,
                    quantity: 1,
                    finish: "foil".into(),
                    condition: Condition::LightPlay,
                    language: "en".into(),
                    price_paid: 0.11,
                    acquired_date: "2018-08-08".into(),
                    ..Row::default()
                },
            ),
            (
                "Moxfield",
                "\"Count\",\"Tradelist Count\",\"Name\",\"Edition\",\"Condition\",\"Language\",\"Foil\",\"Tags\",\"Last Modified\",\"Collector Number\",\"Alter\",\"Proxy\",\"Purchase Price\"\n\
                 \"3\",\"0\",\"Lightning Bolt\",\"2xm\",\"Near Mint\",\"English\",\"foil\",\"\",\"2024-01-02 10:00:00.000000\",\"117\",\"False\",\"False\",\"1.50\"\n",
                Row {
                    name: "Lightning Bolt".into(),
                    set_code: "2xm".into(),
                    number: "117".into(),
                    quantity: 3,
                    finish: "foil".into(),
                    condition: Condition::NearMint,
                    language: "en".into(),
                    price_paid: 1.5,
                    ..Row::default()
                },
            ),
            (
                "ManaBox",
                "Name,Set code,Set name,Collector number,Foil,Rarity,Quantity,ManaBox ID,Scryfall ID,Purchase price,Misprint,Altered,Condition,Language,Purchase price currency\n\
                 Sol Ring,C21,Commander 2021,263,normal,uncommon,2,1,0afa0e33-4804-4b00-b625-c2d6b61090fc,2.00,false,false,near_mint,ja,USD\n",
                Row {
                    name: "Sol Ring".into(),
                    set_code: "c21".into(),
                    set_name: "Commander 2021".into(),
                    number: "263".into(),
                    quantity: 2,
                    scryfall_id: "0afa0e33-4804-4b00-b625-c2d6b61090fc".into(),
                    finish: "nonfoil".into(),
                    condition: Condition::NearMint,
                    language: "ja".into(),
                    price_paid: 2.0,
                    ..Row::default()
                },
            ),
            (
                "Deckbox",
                "Count,Tradelist Count,Name,Edition,Edition Code,Card Number,Condition,Language,Foil,Signed,Artist Proof,Altered Art,Misprint,Promo,Textless,My Price\n\
                 1,0,Counterspell,Mirage,MIR,,Played,English,,,,,,,,$4.00\n",
                Row {
                    name: "Counterspell".into(),
                    set_code: "mir".into(),
                    set_name: "Mirage".into(),
                    quantity: 1,
                    finish: "nonfoil".into(),
                    condition: Condition::Moderate,
                    language: "en".into(),
                    ..Row::default()
                },
            ),
            (
                "TCGplayer",
                "Quantity,Name,Simple Name,Set,Card Number,Set Code,Printing,Condition,Language,Rarity,Product ID,SKU\n\
                 4,Llanowar Elves,Llanowar Elves,Dominaria,168,DOM,Normal,Near Mint,English,C,160000,123\n",
                Row {
                    name: "Llanowar Elves".into(),
                    set_code: "dom".into(),
                    set_name: "Dominaria".into(),
                    number: "168".into(),
                    quantity: 4,
                    tcgplayer_id: 160000,
                    finish: "nonfoil".into(),
                    condition: Condition::NearMint,
                    language: "en".into(),
                    ..Row::default()
                },
            ),
            (
                "Dragon Shield",
                "sep=,\nFolder Name,Quantity,Trade Quantity,Card Name,Set Code,Set Name,Card Number,Condition,Printing,Language,Price Bought,Date Bought,LOW,MID,MARKET\n\
                 Binder,1,0,Shock,M19,Core Set 2019,156,Excellent,Foil,English,0.25,2019-03-04,0.1,0.2,0.3\n",
                Row {
                    name: "Shock".into(),
                    set_code: "m19".into(),
                    set_name: "Core Set 2019".into(),
                    number: "156".into(),
                    quantity: 1,
                    finish: "foil".into(),
                    condition: Condition::LightPlay,
                    language: "en".into(),
                    price_paid: 0.25,
                    acquired_date: "2019-03-04".into(),
                    ..Row::default()
                },
            ),
        ];
        for (format, csv, want) in cases {
            let parsed = parse_csv(csv.as_bytes()).unwrap();
            assert_eq!(&parsed.format, format);
            assert_eq!(parsed.rows.len(), 1, "{format}");
            assert_eq!(row(parsed.rows[0].clone()), *want, "{format}");
            assert!(parsed.warnings.is_empty(), "{format}: {:?}", parsed.warnings);
        }
    }

    /// An export with EchoMTG's real header and made-up rows.
    const ECHOMTG: &[u8] = include_bytes!("../../tests/echomtg-export.txt");

    #[test]
    fn echomtg_export() {
        let parsed = parse_csv(ECHOMTG).unwrap();
        assert_eq!(parsed.format, "EchoMTG");
        assert!(parsed.warnings.is_empty(), "one row per copy is expected: {:?}", parsed.warnings);
        assert_eq!(parsed.header, std::str::from_utf8(ECHOMTG).unwrap().lines().next().unwrap());
        assert_eq!(parsed.rows.len(), 6);
        assert!(parsed.rows.iter().all(|r| r.quantity == 1 && r.language == "en"));
        let foil = &parsed.rows[1];
        assert_eq!(
            (foil.name.as_str(), foil.set_code.as_str(), foil.number.as_str(), foil.finish.as_str()),
            ("Shock", "m19", "156", "foil")
        );
        assert_eq!((foil.condition, foil.price_paid, foil.acquired_date.as_str()), (Condition::LightPlay, 1.1, "2018-08-16"));
        assert_eq!((foil.tcgplayer_id, foil.line), (70001, 3));
        assert_eq!(parsed.rows[2].condition, Condition::Moderate);
        assert_eq!(parsed.rows[3].name, "Lightning Bolt (1)");
        assert_eq!(parsed.rows.iter().filter(|r| r.finish == "foil").count(), 1);
        assert_eq!(parsed.rows[0].text, std::str::from_utf8(ECHOMTG).unwrap().lines().nth(1).unwrap());
        assert!(parsed.rows.iter().all(|r| r.scryfall_id.is_empty() && r.note.is_empty()));
    }

    #[test]
    fn split_quantities_and_notes() {
        let parsed = parse_csv(b"Name,Reg Qty,Foil Qty,Finish,Notes\nBolt,2,1,etched,binder A\nOpt,0,0,,\nShock,0,3,,\n").unwrap();
        let got: Vec<_> = parsed.rows.iter().map(|r| (r.name.as_str(), r.quantity, r.finish.as_str(), r.note.as_str(), r.line)).collect();
        assert_eq!(got, [("Bolt", 2, "nonfoil", "binder A", 2), ("Bolt", 1, "foil", "binder A", 2), ("Shock", 3, "foil", "", 4)]);
        assert!(parsed.warnings.is_empty());
    }

    #[test]
    fn warnings_name_the_columns() {
        let parsed = parse_csv(b"Name,Price Paid,Acquired\nA,abc,someday\nB,xyz,2024-01-01\nC,,\n").unwrap();
        assert_eq!(parsed.rows.len(), 3);
        assert_eq!(
            parsed.warnings,
            [
                "No quantity column was found, so every row was counted as 1 copy.",
                "The price column \"Price Paid\" couldn't be read in 2 of 2 rows; those were left blank.",
            ]
        );
        let ok = parse_csv(b"Name,Quantity,Price Paid\nA,1,0.50\n").unwrap();
        assert!(ok.warnings.is_empty());
    }

    #[test]
    fn profile_overrides_come_first() {
        let p = Profile { overrides: &[(Field::Price, &["acquired"])], ..Profile::new("Test", |_| true) };
        assert_eq!(p.field("acquired"), Some(Field::Price));
        assert_eq!(p.field("date acquired"), Some(Field::Date));
        assert_eq!(Profile::new("Plain", |_| true).field("acquired"), Some(Field::Date));
        assert_eq!(p.field("note id"), None);
    }

    #[test]
    fn lines_zero_quantities_and_unknown_files() {
        let rows = parse_csv(b"Name,Quantity\nOpt,2\nWish,0\n\"Shock\",1\n").unwrap().rows;
        assert_eq!(rows.iter().map(|r| (r.name.as_str(), r.line)).collect::<Vec<_>>(), [("Opt", 2), ("Shock", 4)]);
        assert_eq!(rows[0].describe(), "Line 2: Opt");
        assert!(parse_csv(b"foo,bar\n1,2\n").is_err());
        assert!(parse_csv(b"").is_err());
    }

    #[test]
    fn loose_values() {
        assert_eq!(parse_int_loose(" 3.0 "), 3);
        assert_eq!(parse_int_loose("x"), 0);
        assert_eq!(parse_price("$1,234.50"), 1234.5);
        assert_eq!(parse_price("2,50 €"), 2.5);
        assert_eq!(parse_price("-1"), 0.0);
        for (raw, want) in [
            ("2024-03-15", "2024-03-15"),
            ("2024/03/15", "2024-03-15"),
            ("3/15/2024", "2024-03-15"),
            ("03/15/24", "2024-03-15"),
            ("Mar 15, 2024", "2024-03-15"),
            ("15 Mar 2024", "2024-03-15"),
            ("2024-03-15 10:00:00", "2024-03-15"),
            ("2024-03-15T10:00:00.000Z", "2024-03-15"),
            ("2024-03-15T23:00:00-05:00", "2024-03-15"),
            ("2018-08-08 00:00:00 UTC", "2018-08-08"),
            ("someday", ""),
        ] {
            assert_eq!(parse_date(raw), want, "{raw}");
        }
    }
}
