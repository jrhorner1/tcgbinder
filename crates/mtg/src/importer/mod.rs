//! Reading other collection apps' CSV exports (EchoMTG, ManaBox, Moxfield, Deckbox,
//! TCGplayer, Dragon Shield, Archidekt, Delver Lens and generic files) and matching
//! each row to its exact Scryfall printing.

mod formats;
mod resolve;

pub use formats::{Row, parse_csv};
pub use resolve::{front_face, resolve};

/// The finish to record for a row: the one asked for when the printing comes in it (or
/// nothing is known), else the printing's only finish.
pub fn choose_finish(want: &str, available: &[String]) -> String {
    match available {
        [] => want.to_string(),
        _ if available.iter().any(|f| f == want) => want.to_string(),
        [only] => only.clone(),
        _ => want.to_string(),
    }
}

/// Normalizes foil markers from any export to Scryfall's finish names.
pub fn parse_finish(raw: &str) -> &'static str {
    let clean = raw.trim().to_lowercase();
    if clean.contains("etched") {
        "etched"
    } else if clean.contains("non") || clean.contains("normal") || clean.contains("regular") {
        "nonfoil"
    } else if clean.contains("foil") || matches!(clean.as_str(), "1" | "1.0" | "true" | "yes" | "y" | "f" | "x") {
        "foil"
    } else {
        "nonfoil"
    }
}

/// Maps language names and codes to Scryfall's codes, defaulting to English.
pub fn parse_language(raw: &str) -> &'static str {
    match raw.trim().to_lowercase().as_str() {
        "es" | "sp" | "spanish" => "es",
        "fr" | "french" => "fr",
        "de" | "german" => "de",
        "it" | "italian" => "it",
        "pt" | "portuguese" | "portuguese (brazil)" => "pt",
        "ja" | "jp" | "japanese" => "ja",
        "ko" | "kr" | "korean" => "ko",
        "ru" | "russian" => "ru",
        "zhs" | "cs" | "zh" | "simplified chinese" | "chinese simplified" | "chinese" => "zhs",
        "zht" | "ct" | "traditional chinese" | "chinese traditional" => "zht",
        "he" | "hebrew" => "he",
        "la" | "latin" => "la",
        "grc" | "ancient greek" => "grc",
        "ar" | "arabic" => "ar",
        "sa" | "sanskrit" => "sa",
        "ph" | "phyrexian" => "ph",
        _ => "en",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finishes_and_languages() {
        let v = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(choose_finish("foil", &v(&["nonfoil", "foil"])), "foil");
        assert_eq!(choose_finish("foil", &v(&["nonfoil"])), "nonfoil");
        assert_eq!(choose_finish("etched", &v(&["nonfoil", "foil"])), "etched");
        assert_eq!(choose_finish("foil", &[]), "foil");
        for (raw, want) in
            [("Foil", "foil"), ("Non-Foil", "nonfoil"), ("etched foil", "etched"), ("1.0", "foil"), ("", "nonfoil"), ("Normal", "nonfoil")]
        {
            assert_eq!(parse_finish(raw), want, "{raw}");
        }
        assert_eq!(parse_language("Japanese"), "ja");
        assert_eq!(parse_language("Klingon"), "en");
    }
}
