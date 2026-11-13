//! Card scanning. Scryfall has no image search, so a scan is identified from the text on
//! the card: the name in the title bar and, on cards printed since 2014 (Magic 2015),
//! the set code, language and collector number in the bottom-left corner, e.g.
//!
//! ```text
//! 0156/280 C          or, since 2023,   0012 U
//! M19 • EN  ⟩ ARTIST                    LCI • EN ⟩ ARTIST
//! ```
//!
//! The set code and number identify the exact printing; the name confirms it, and is
//! all there is to go on for older cards.

use std::sync::LazyLock;

use anyhow::Result;
use regex::Regex;

use tcgbinder_core::collection::Card;
use tcgbinder_core::game::{Game, ScanResult, Scanner};
use tcgbinder_core::scan::{Line, Reading};

use crate::{Magic, from_scryfall};

/// What a reading says about the card.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Clues {
    name: String,
    set: String,
    number: String,
    lang: String,
    /// The copyright year, e.g. 1995 from "© 1995 Wizards of the Coast".
    year: i32,
}

impl Clues {
    /// How much a reading identifies (for choosing which way up the card is).
    fn score(&self) -> u32 {
        u32::from(!self.name.is_empty()) + 2 * u32::from(!self.set.is_empty()) + 2 * u32::from(!self.number.is_empty())
    }

    fn key(&self) -> String {
        format!("{}|{}|{}", self.name.to_lowercase(), self.set, self.number)
    }

    /// What was read, e.g. "Shock · M19 · 156".
    fn describe(&self) -> String {
        let mut parts: Vec<String> =
            [&self.name, &self.set, &self.number].iter().filter(|p| !p.is_empty()).map(|p| p.to_string()).collect();
        if !self.lang.is_empty() && self.lang != "en" {
            parts.push(self.lang.to_uppercase());
        }
        if parts.is_empty() { "nothing readable".into() } else { parts.join(" · ") }
    }
}

fn re(s: &str) -> Regex {
    Regex::new(s).expect("valid regex")
}

/// "M19 • EN", "LCI · EN", "WAR*JP": a set code, a separator and a language code.
static SET_LANG: LazyLock<Regex> =
    LazyLock::new(|| re(r"\b([A-Z0-9]{3,5})\s*[^A-Za-z0-9\s]{0,3}\s*(EN|ES|FR|DE|IT|PT|JP|JA|KR|KO|RU|CS|CT|ZHS|ZHT|PH)\b"));
/// "0156/280": the number of the set's total (2014-2022 frames, and 2008-2014 cards).
static NUMBER_OF: LazyLock<Regex> = LazyLock::new(|| re(r"\b([0-9]{1,4})\s*/\s*[0-9]{1,4}\b"));
/// "0012 U": the number then the rarity letter (frames since 2023).
static NUMBER_RARITY: LazyLock<Regex> = LazyLock::new(|| re(r"(?:^|\s)([A-Z]?[0-9]{1,4}[a-z]?)\s+[CURMLSTPB](?:\s|$)"));
/// "U 0260": the rarity letter then the number (some frames since 2023).
static RARITY_NUMBER: LazyLock<Regex> = LazyLock::new(|| re(r"(?:^|\s)[CURMLSTPB]\s+([0-9]{3,4}[a-z]?)(?:\s|$)"));
static FOOTER: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)wizards of the coast|©|illus\.|all rights reserved"));
static YEAR: LazyLock<Regex> = LazyLock::new(|| re(r"(?:©|\(c\)|\bC)\s*((?:19|20)\d\d)\b|\b((?:19|20)\d\d)\s+Wizards"));

/// The language codes printed on cards, as Scryfall's.
fn lang_code(printed: &str) -> &'static str {
    match printed {
        "EN" => "en",
        "ES" => "es",
        "FR" => "fr",
        "DE" => "de",
        "IT" => "it",
        "PT" => "pt",
        "JP" | "JA" => "ja",
        "KR" | "KO" => "ko",
        "RU" => "ru",
        "CS" | "ZHS" => "zhs",
        "CT" | "ZHT" => "zht",
        "PH" => "ph",
        _ => "",
    }
}

/// Picks the name, set, language and collector number out of a reading. A card
/// photographed upside down reads fine but its lines come back in flipped places, so
/// the flipped layout is tried too.
fn read_clues(r: &Reading) -> Clues {
    let c = clues_as_is(r);
    if !r.card_found {
        return c;
    }
    let flipped = clues_as_is(&flip(r));
    if flipped.score() > c.score() || (flipped.score() == c.score() && footer_on_top(r)) { flipped } else { c }
}

/// Turns a card reading's positions upside down.
fn flip(r: &Reading) -> Reading {
    let lines = r.lines.iter().map(|l| Line { x: 1.0 - l.x - l.w, y: 1.0 - l.y - l.h, ..l.clone() }).collect();
    Reading { lines, card_found: r.card_found }
}

/// Whether the copyright or artist line, printed at the bottom of every card, was read
/// in the top half: the card is upside down.
fn footer_on_top(r: &Reading) -> bool {
    r.lines.iter().find(|l| FOOTER.is_match(&l.text)).is_some_and(|l| l.y < 0.5)
}

fn clues_as_is(r: &Reading) -> Clues {
    let mut c = Clues::default();
    let lines = r.sorted();

    // The name: the biggest text near the top-left of the card (when the card was
    // found), otherwise the first line that looks like a name.
    let mut best: Option<&Line> = None;
    for l in &lines {
        if letter_count(&clean_name(&l.text)) < 3 {
            continue;
        }
        if r.card_found {
            if l.y + l.h / 2.0 > 0.14 || l.x > 0.5 {
                continue;
            }
            if best.is_none_or(|b| l.h > b.h * 1.15) {
                best = Some(l);
            }
        } else if best.is_none() {
            best = Some(l);
        }
    }
    if let Some(b) = best {
        c.name = clean_name(&b.text);
    }

    for l in &lines {
        if c.year == 0
            && let Some(m) = YEAR.captures(&l.text)
        {
            c.year = m.get(1).or(m.get(2)).and_then(|y| y.as_str().parse().ok()).unwrap_or(0);
        }
    }
    // The set, language and number: in the bottom-left corner of the card.
    for l in lines.iter().filter(|l| !r.card_found || (l.y >= 0.85 && l.x <= 0.6)) {
        if c.set.is_empty()
            && let Some(m) = SET_LANG.captures(&l.text)
            && !m[1].bytes().all(|b| b.is_ascii_digit())
        {
            c.set = m[1].to_string();
            c.lang = lang_code(&m[2]).to_string();
        }
        if c.number.is_empty()
            && let Some(m) = [&NUMBER_OF, &NUMBER_RARITY, &RARITY_NUMBER].iter().find_map(|re| re.captures(&l.text))
        {
            c.number = trim_zeros(&m[1]);
        }
    }
    c
}

/// Keeps the letters, spaces and punctuation names use, dropping what OCR makes of mana
/// symbols (e.g. "Shock ®" → "Shock").
fn clean_name(s: &str) -> String {
    let kept: String = s
        .chars()
        .map(|r| match r {
            '’' => '\'',
            r if r.is_alphabetic() || matches!(r, ' ' | '\'' | ',' | '-') => r,
            _ => ' ',
        })
        .collect();
    let mut words: Vec<&str> = kept.split_whitespace().collect();
    while words.len() > 1 && words.last().is_some_and(|w| w.chars().count() == 1) {
        words.pop(); // stray symbol letters at the end
    }
    while words.first().is_some_and(|w| w.trim_matches(|c| matches!(c, ',' | '\'' | '-')).is_empty()) {
        words.remove(0);
    }
    words.join(" ").trim_end_matches([',', '-', '\'', ' ']).to_string()
}

fn letter_count(s: &str) -> usize {
    s.chars().filter(|c| c.is_alphabetic()).count()
}

/// Drops the leading zeros printed on cards ("0156" → "156"), as Scryfall's collector
/// numbers have none.
fn trim_zeros(n: &str) -> String {
    let t = n.trim_start_matches('0');
    if t.is_empty() { "0".into() } else { t.into() }
}

/// The set code as read and, when OCR may have confused letters and digits (O/0, I/1,
/// ...), single-character corrections to try next.
fn set_variants(code: &str) -> Vec<String> {
    let mut out = vec![code.to_string()];
    for (i, ch) in code.char_indices() {
        if out.len() >= 4 {
            break;
        }
        let to = match ch {
            'O' => '0',
            '0' => 'O',
            'I' | 'L' => '1',
            '1' => 'I',
            'S' => '5',
            '5' => 'S',
            'B' => '8',
            '8' => 'B',
            'Z' => '2',
            '2' => 'Z',
            _ => continue,
        };
        out.push(format!("{}{to}{}", &code[..i], &code[i + ch.len_utf8()..]));
    }
    out
}

impl Scanner for Magic {
    fn scan_key(&self, r: &Reading) -> (String, bool) {
        let c = read_clues(r);
        let complete = !c.name.is_empty() || (!c.set.is_empty() && !c.number.is_empty());
        (c.key(), complete)
    }

    /// The printing by set and collector number when they were read (checked against
    /// the name), otherwise every printing of the card whose name is closest to the one
    /// read.
    fn identify(&self, r: &Reading) -> Result<ScanResult> {
        let c = read_clues(r);
        let mut res = ScanResult { read: c.describe(), name: c.name.clone(), ..Default::default() };
        tracing::debug!(name = c.name, set = c.set, number = c.number, lang = c.lang, "scan clues");

        if !c.set.is_empty() && !c.number.is_empty() {
            for set in set_variants(&c.set) {
                let Some(sc) = self.scryfall.card_by_number(&set, &c.number, &c.lang)? else { continue };
                let card = from_scryfall(&sc);
                // Names on cards in other languages are translated, so they can't be checked.
                if c.name.is_empty() || (!c.lang.is_empty() && c.lang != "en") || same_name(&c.name, &card.name) {
                    if c.name.is_empty() {
                        res.note = "The name couldn't be read; this is the card with that set and collector number.".into();
                    }
                    res.name = card.name.clone();
                    res.candidates = vec![card];
                    res.exact = true;
                    return Ok(res);
                }
                tracing::debug!(read = c.name, found = card.name, "scan: the set and number disagree with the name");
                break; // trust the name over a misread number
            }
        }

        if c.name.is_empty() {
            res.note = "The card's name couldn't be read. Try again with the whole card in view and less glare.".into();
            return Ok(res);
        }
        let Some(sc) = self.scryfall.named_fuzzy(&c.name)? else {
            res.note = format!("No card is named like {:?}. Try again, or search by hand.", c.name);
            return Ok(res);
        };
        res.name = sc.name.clone();
        let mut printings = self.printings(&from_scryfall(&sc))?;
        if printings.is_empty() {
            printings = vec![from_scryfall(&sc)];
        }
        // Printings whose set and number agree with what was read come first, then (for
        // older cards, which print neither) those released nearest the copyright year.
        let score = |p: &Card| {
            2 * u32::from(!c.set.is_empty() && p.set.eq_ignore_ascii_case(&c.set))
                + u32::from(!c.number.is_empty() && p.collector_number == c.number)
        };
        let year_gap = |p: &Card| -> i32 {
            match (c.year, p.released_at.get(..4).and_then(|y| y.parse::<i32>().ok())) {
                (0, _) | (_, None) => 0,
                (year, Some(y)) => (y - year).abs(),
            }
        };
        // The name was read, so it's in the alphabet of an English card.
        let lang = if c.lang.is_empty() { "en" } else { &c.lang };
        let other_lang = |p: &Card| !p.language.is_empty() && p.language != lang;
        printings.sort_by(|a, b| {
            score(b).cmp(&score(a)).then_with(|| other_lang(a).cmp(&other_lang(b))).then_with(|| year_gap(a).cmp(&year_gap(b)))
        });
        let top = score(&printings[0]);
        res.exact = top == 3 || printings.len() == 1;
        if !res.exact {
            res.note = if c.set.is_empty() && c.number.is_empty() {
                if c.year != 0 {
                    format!("Cards before 2014 don't print their set; printings from around {} are listed first. Check yours.", c.year)
                } else {
                    "The set and collector number couldn't be read (cards before 2014 don't print them), so pick your printing.".into()
                }
            } else if top >= 2 {
                "The collector number couldn't be confirmed; check the printing.".into()
            } else {
                "The set couldn't be matched; pick your printing.".into()
            };
        }
        res.candidates = printings;
        Ok(res)
    }
}

/// Whether a name read by OCR is close enough to a card's name (or the name of one of
/// its faces).
fn same_name(read: &str, name: &str) -> bool {
    let a = norm_name(read);
    name.split(" // ").any(|face| {
        let b = norm_name(face);
        let longest = a.chars().count().max(b.chars().count());
        a == b || (a.chars().count() >= 4 && (b.contains(&a) || a.contains(&b))) || levenshtein(&a, &b) as f64 <= 0.25 * longest as f64
    })
}

fn norm_name(s: &str) -> String {
    s.to_lowercase().chars().filter(|c| c.is_alphabetic()).collect()
}

fn levenshtein(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1; b.len() + 1];
        for (j, cb) in b.iter().enumerate() {
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + usize::from(ca != cb));
        }
        prev = cur;
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scryfall;
    use crate::testing::{FakeServer, Reply};

    /// A reading of a card found and straightened in the photo: the title, the type
    /// line, and the bottom-left info lines.
    fn card_reading(title: &str, info: &[&str]) -> Reading {
        let line = |text: &str, x, y, w, h| Line { text: text.into(), x, y, w, h, confidence: 1.0 };
        let mut lines = vec![
            line(title, 0.07, 0.045, 0.5, 0.04),
            line("{2}", 0.85, 0.045, 0.08, 0.04), // mana cost
            line("Instant", 0.07, 0.56, 0.3, 0.035),
            line("Shock deals 2 damage to any target.", 0.08, 0.64, 0.8, 0.03),
            line("™ & © 2018 Wizards of the Coast", 0.6, 0.95, 0.35, 0.015),
        ];
        for (i, t) in info.iter().enumerate() {
            lines.push(line(t, 0.05, 0.93 + i as f64 * 0.025, 0.35, 0.015));
        }
        Reading { lines, card_found: true }
    }

    fn clues(name: &str, set: &str, number: &str, lang: &str, year: i32) -> Clues {
        Clues { name: name.into(), set: set.into(), number: number.into(), lang: lang.into(), year }
    }

    #[test]
    fn reads_clues() {
        let unfound = Reading {
            lines: ["Shock", "Instant", "156/280 C", "M19 • EN"]
                .iter()
                .zip([0.2, 0.5, 0.8, 0.82])
                .map(|(t, y)| Line { text: t.to_string(), y, ..Default::default() })
                .collect(),
            card_found: false,
        };
        for (what, r, want) in [
            ("2014-2022 frame", card_reading("Shock", &["0156/280 C", "M19 • EN ⟩ JUNG PARK"]), clues("Shock", "M19", "156", "en", 2018)),
            (
                "2023 frame",
                card_reading("Scampering Surveyor", &["0260 C", "LCI • EN SARAH FINNIGAN"]),
                clues("Scampering Surveyor", "LCI", "260", "en", 2018),
            ),
            (
                "rarity first",
                card_reading("Scampering Surveyor", &["U 0260", "LCI • EN IO XAVIER RIBEIRO"]),
                clues("Scampering Surveyor", "LCI", "260", "en", 2018),
            ),
            ("mana symbols and Japanese", card_reading("Opt ® ⓦ", &["060/269 C", "DOM * JP"]), clues("Opt", "DOM", "60", "ja", 2018)),
            ("curly apostrophe", card_reading("Urza’s Ruinous Blast", &[]), clues("Urza's Ruinous Blast", "", "", "", 2018)),
            (
                "older card: name and year",
                card_reading("Llanowar Elves", &["Illus. Anson Maddocks"]),
                clues("Llanowar Elves", "", "", "", 2018),
            ),
            ("upside down", flip(&card_reading("Shock", &["0156/280 C", "M19 • EN"])), clues("Shock", "M19", "156", "en", 2018)),
            ("no outline found", unfound, clues("Shock", "M19", "156", "en", 0)),
        ] {
            assert_eq!(read_clues(&r), want, "{what}");
        }
    }

    #[test]
    fn scan_key_needs_something_to_look_up() {
        let g = Magic::default();
        let instant = Reading { lines: vec![Line { text: "Instant".into(), y: 0.56, h: 0.03, ..Default::default() }], card_found: true };
        assert!(!g.scan_key(&instant).1, "a reading without a name or number shouldn't be complete");
        assert_eq!(g.scan_key(&card_reading("Shock", &["0156/280 C", "M19 • EN"])), ("shock|M19|156".into(), true));
    }

    #[test]
    fn names_and_variants() {
        for (read, name) in [
            ("Shock", "Shock"),
            ("Lightning Bo1t", "Lightning Bolt"),
            ("Delver of Secrets", "Delver of Secrets // Insectile Aberration"),
            ("Scapeshitt", "Scapeshift"),
        ] {
            assert!(same_name(read, name), "{read:?} should match {name:?}");
        }
        assert!(!same_name("Shock", "Opt") && !same_name("Lightning Bolt", "Lightning Helix"));
        assert_eq!(set_variants("MI9"), ["MI9", "M19"]);
        assert_eq!(levenshtein("kitten", "sitting"), 3);
    }

    /// Serves /cards/{set}/{number}, /cards/named?fuzzy= and the printings search.
    fn fake_scryfall() -> (Magic, FakeServer) {
        let shock = |set: &str, num: &str| {
            let released = match set {
                "m19" => "2018-07-13",
                "2xm" => "2020-08-07",
                _ => "2017-04-28",
            };
            scryfall::Card {
                id: format!("id-{set}-{num}"),
                oracle_id: "o-shock".into(),
                name: "Shock".into(),
                set: set.into(),
                set_name: set.to_uppercase(),
                collector_number: num.into(),
                rarity: "common".into(),
                finishes: vec!["nonfoil".into(), "foil".into()],
                released_at: released.into(),
                ..Default::default()
            }
        };
        let srv = FakeServer::start(move |req| match req.path.as_str() {
            "/cards/m19/156" => Reply::json(&shock("m19", "156")),
            "/cards/named" if req.query.get("fuzzy").is_some_and(|f| f.eq_ignore_ascii_case("shok")) => Reply::json(&shock("akh", "144")),
            "/cards/search" if req.query.get("q").map(String::as_str) == Some("oracleid:o-shock") => {
                Reply::json(&scryfall::SearchResponse {
                    data: vec![shock("2xm", "141"), shock("m19", "156"), shock("akh", "144")],
                    total_cards: 3,
                    ..Default::default()
                })
            }
            _ => Reply::not_found(),
        });
        let g = Magic::with_clients(scryfall::Client::unlimited(&srv.url), crate::mtgjson::Client::new(&srv.url));
        (g, srv)
    }

    #[test]
    fn identifies_cards() {
        let (g, srv) = fake_scryfall();

        // Set and number read, name agrees: exactly that printing, in one request.
        let res = g.identify(&card_reading("Shock", &["0156/280 C", "M19 • EN"])).unwrap();
        assert!(res.exact && res.candidates.len() == 1 && res.candidates[0].id == "id-m19-156");
        assert_eq!(srv.calls.lock().unwrap().len(), 1);

        // The set code misread as "MI9": the corrected code is tried.
        let res = g.identify(&card_reading("Shock", &["0156/280 C", "MI9 • EN"])).unwrap();
        assert!(res.exact && res.candidates[0].id == "id-m19-156");

        // Only a misspelled name: a fuzzy match, then every printing to choose from.
        let res = g.identify(&card_reading("Shok", &[])).unwrap();
        assert!(!res.exact && res.candidates.len() == 3 && res.name == "Shock" && !res.note.is_empty());

        // The name plus a set, with the number unreadable: that set's printing comes first.
        let res = g.identify(&card_reading("Shok", &["", "AKH • EN"])).unwrap();
        assert_eq!(res.candidates[0].set, "AKH");

        // Nothing that matches.
        let res = g.identify(&card_reading("Xyzzy Plugh", &[])).unwrap();
        assert!(res.candidates.is_empty() && res.note.contains("Xyzzy Plugh"));
    }
}
