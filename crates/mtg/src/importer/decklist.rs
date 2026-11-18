//! Card lists as people paste them: decklists from Moxfield, Archidekt, MTG Arena and
//! the like, one card per line ("4 Lightning Bolt (M11) 149").

use std::sync::LazyLock;

use regex::Regex;
use tcgbinder_core::Condition;

use super::formats::{ParsedCsv, Row};

/// Lines that head a section of a decklist rather than naming a card.
const HEADINGS: &[&str] = &[
    "deck",
    "main",
    "mainboard",
    "main deck",
    "sideboard",
    "side",
    "commander",
    "commanders",
    "companion",
    "maybeboard",
    "maybe",
    "considering",
    "tokens",
    "about",
];

/// "4 Lightning Bolt", "4x Lightning Bolt", "4 X Lightning Bolt".
static COUNT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\d+)\s*[xX]?\s+(.+)$").unwrap());
/// A foil or etched marker at the end: "*F*", "*E*", "(foil)", "(etched)".
static FINISH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\s*(\*f\*|\*e\*|\(foil\)|\(etched\))$").unwrap());
/// "(M11) 149" or "(M11)" at the end.
static PAREN_SET: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*\(([A-Za-z0-9]{2,8})\)(?:\s+([A-Za-z0-9★†-]+))?$").unwrap());
/// "[M11] 149" or "[M11]" at the end.
static BRACKET_SET: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*\[([A-Za-z0-9]{2,8})\](?:\s+([A-Za-z0-9★†-]+))?$").unwrap());
/// A heading with a count, e.g. "Sideboard (15)" or "Creatures: 20".
static COUNTED_HEADING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(.+?)\s*(?:\(\d+\)|:\s*\d*)$").unwrap());

/// Whether a line heads a section instead of naming a card.
fn is_heading(line: &str) -> bool {
    let lower = line.to_lowercase();
    if HEADINGS.contains(&lower.as_str()) || line.ends_with(':') {
        return true;
    }
    COUNTED_HEADING.captures(&lower).is_some_and(|c| HEADINGS.contains(&c[1].trim()))
}

/// Reads a pasted card list: one card per line, with an optional count, set ("(M19)"
/// or "[M19]") and collector number, and foil marker. Blank lines, comments (`//`, `#`) and section
/// headings ("Sideboard") are skipped; a line without a count is one copy.
pub fn parse_decklist(text: &str) -> ParsedCsv {
    let mut rows = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("//") || line.starts_with('#') || is_heading(line) {
            continue;
        }
        let (quantity, mut rest) = match COUNT.captures(line) {
            Some(c) => (c[1].parse().unwrap_or(0), c.get(2).map_or("", |m| m.as_str()).to_string()),
            None => (1, line.to_string()),
        };
        if quantity <= 0 {
            continue;
        }
        let mut finish = "nonfoil";
        if let Some(m) = FINISH.find(&rest) {
            finish = if m.as_str().to_lowercase().contains('e') && !m.as_str().to_lowercase().contains("foil") { "etched" } else { "foil" };
            rest.truncate(m.start());
        }
        let (mut set_code, mut number) = (String::new(), String::new());
        if let Some(c) = PAREN_SET.captures(&rest) {
            set_code = c[1].to_lowercase();
            number = c.get(2).map_or("", |m| m.as_str()).to_string();
            rest.truncate(c.get(0).unwrap().start());
        } else if let Some(c) = BRACKET_SET.captures(&rest) {
            set_code = c[1].to_lowercase();
            number = c.get(2).map_or("", |m| m.as_str()).to_string();
            rest.truncate(c.get(0).unwrap().start());
        }
        let name = rest.trim().to_string();
        if name.is_empty() {
            continue;
        }
        rows.push(Row {
            line: i as u64 + 1,
            name,
            set_code,
            number,
            quantity,
            finish: finish.into(),
            condition: Condition::NearMint,
            language: "en".into(),
            text: raw.trim_end_matches('\r').to_string(),
            ..Row::default()
        });
    }
    ParsedCsv { rows, format: "Card list".into(), warnings: Vec::new(), header: String::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(line: &str) -> (i64, String, String, String, String) {
        let rows = parse_decklist(line).rows;
        assert_eq!(rows.len(), 1, "{line}");
        let r = &rows[0];
        (r.quantity, r.name.clone(), r.set_code.clone(), r.number.clone(), r.finish.clone())
    }

    #[test]
    fn line_forms() {
        let bolt =
            |q, set: &str, num: &str, fin: &str| (q, "Lightning Bolt".to_string(), set.to_string(), num.to_string(), fin.to_string());
        assert_eq!(one("4 Lightning Bolt"), bolt(4, "", "", "nonfoil"));
        assert_eq!(one("4x Lightning Bolt"), bolt(4, "", "", "nonfoil"));
        assert_eq!(one("4 Lightning Bolt (M11) 149"), bolt(4, "m11", "149", "nonfoil"));
        assert_eq!(one("4 Lightning Bolt [M11]"), bolt(4, "m11", "", "nonfoil"));
        assert_eq!(one("1 Lightning Bolt (M11) 149 *F*"), bolt(1, "m11", "149", "foil"));
        assert_eq!(one("2 Lightning Bolt (foil)"), bolt(2, "", "", "foil"));
        assert_eq!(one("1 Lightning Bolt (2X2) 117 *E*"), bolt(1, "2x2", "117", "etched"));
        assert_eq!(one("Lightning Bolt"), bolt(1, "", "", "nonfoil"));
        assert_eq!(one("5 Plains [M19] 261"), (5, "Plains".to_string(), "m19".to_string(), "261".to_string(), "nonfoil".to_string()));
        assert_eq!(one("1 Fire // Ice (MH2) 290"), (1, "Fire // Ice".into(), "mh2".into(), "290".into(), "nonfoil".into()));
    }

    #[test]
    fn skips_headings_comments_and_blanks() {
        let list = "Deck\n4 Lightning Bolt\n\n// burn\n# notes\nSideboard (15)\n2 Shock\nSideboard:\nCommander\n1 Sol Ring\n0 Opt\n";
        let rows = parse_decklist(list).rows;
        let got: Vec<(u64, &str, i64)> = rows.iter().map(|r| (r.line, r.name.as_str(), r.quantity)).collect();
        assert_eq!(got, [(2, "Lightning Bolt", 4), (7, "Shock", 2), (10, "Sol Ring", 1)]);
        assert_eq!(rows[1].text, "2 Shock");
    }
}
