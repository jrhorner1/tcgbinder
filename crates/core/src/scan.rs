//! What card scanning shares across games and platforms: the text read from a photo of
//! a card, with where each line sits on the card. Reading the text is platform code
//! (the `tcgbinder-vision` crate, Apple's Vision framework); working out which card it
//! is belongs to each game ([`crate::game::Scanner`]).

use serde::{Deserialize, Serialize};

/// One line of text read from a card. The box is normalized, with 0,0 at the top-left
/// and 1,1 at the bottom-right of the card, or of the whole photo when no card outline
/// was found.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Line {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub confidence: f64,
}

impl Line {
    /// The line's lower edge.
    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }
}

/// The text read from one photo or camera frame.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Reading {
    pub lines: Vec<Line>,
    /// The card's outline was found and the photo straightened to it, so positions are
    /// relative to the card itself.
    pub card_found: bool,
}

impl Reading {
    /// The lines top to bottom, then left to right.
    pub fn sorted(&self) -> Vec<Line> {
        let mut out = self.lines.clone();
        // Lines within 1% of the card's height are on the same row. Insertion order is
        // kept for ties (a stable sort), as this isn't a strict ordering.
        out.sort_by(|a, b| {
            let d = a.y - b.y;
            if d.abs() > 0.01 { d.total_cmp(&0.0) } else { a.x.total_cmp(&b.x) }
        });
        out
    }

    /// Every line, top to bottom, one per row (for logs and debugging).
    pub fn text(&self) -> String {
        self.sorted().iter().map(|l| format!("{}\n", l.text)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorted_by_row_then_column() {
        let line = |text: &str, x, y| Line { text: text.into(), x, y, ..Default::default() };
        let r: Reading = serde_json::from_str(r#"{"lines":[],"card_found":true}"#).unwrap();
        assert!(r.card_found);
        let r = Reading { lines: vec![line("b", 0.5, 0.105), line("c", 0.1, 0.9), line("a", 0.1, 0.1)], card_found: true };
        assert_eq!(r.text(), "a\nb\nc\n");
    }
}
