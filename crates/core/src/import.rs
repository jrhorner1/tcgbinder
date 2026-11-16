//! Importing a file into a game's collection: a CSV export from another app (read by
//! the game's importer) or a collection file this app wrote, first as a preview of what
//! it would add, then saved.

use std::fmt::Write as _;

use anyhow::{Result, bail};

use crate::collection::{Card, read_export};
use crate::game::{Game, ImportReport};
use crate::service::GameService;

/// Names collection files written by this app in import reports.
pub const APP_FILE_FORMAT: &str = "TCG Binder collection file";

/// What importing a file would do, with the cards it would add.
#[derive(Clone, Debug, Default)]
pub struct ImportPreview {
    pub cards: Vec<Card>,
    pub report: ImportReport,
    /// Copies to import.
    pub copies: i64,
    /// Cards (printings) not yet owned.
    pub new_cards: usize,
    /// Cards that would add copies to ones already owned.
    pub already_owned: usize,
    /// The price paid for every copy with a known price.
    pub price_paid: f64,
}

impl ImportPreview {
    /// The number of distinct printings to import.
    pub fn printings(&self) -> usize {
        self.cards.iter().map(|c| c.id.as_str()).collect::<std::collections::HashSet<_>>().len()
    }
}

/// A summary of an import.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImportResult {
    pub report: ImportReport,
    /// Copies imported.
    pub cards: i64,
    pub new_cards: usize,
    pub already_owned: usize,
    /// The backup of the collection taken just before (its name), for undo.
    pub backup: String,
}

impl ImportResult {
    /// The result for people.
    pub fn summary(&self) -> String {
        let mut b = String::new();
        let _ = writeln!(b, "Format: {}", self.report.format);
        let read = if self.report.format == APP_FILE_FORMAT { "Cards read" } else { "Rows read" };
        let _ = writeln!(b, "{read}: {}", self.report.rows);
        let _ = writeln!(
            b,
            "Cards imported: {} ({} new cards, {} added to cards you already own)",
            self.cards, self.new_cards, self.already_owned
        );
        if !self.report.notes.is_empty() {
            b.push('\n');
        }
        for n in &self.report.notes {
            let _ = writeln!(b, "{n}");
        }
        b
    }
}

/// Whether `data` looks like a JSON collection file rather than CSV.
pub fn is_app_file(data: &[u8]) -> bool {
    let data = data.strip_prefix("\u{feff}".as_bytes()).unwrap_or(data);
    data.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'{')
}

/// Turns a file into cards for `game` without saving them. App collection files already
/// carry exact catalog IDs and prices, so they are used as-is; anything else goes to
/// the game's CSV importer.
pub fn parse_import(game: &dyn Game, data: &[u8], progress: &mut dyn FnMut(usize, usize)) -> Result<(Vec<Card>, ImportReport)> {
    if !is_app_file(data) {
        return game.import(data, progress);
    }
    let (file_game, cards) = read_export(data)?;
    if file_game != game.id() {
        bail!("this file is a {file_game:?} collection; it can't be imported into {}", game.name());
    }
    if cards.is_empty() {
        bail!("no cards found in this collection file");
    }
    let report = ImportReport { format: APP_FILE_FORMAT.into(), rows: cards.len(), ..Default::default() };
    Ok((cards, report))
}

impl GameService {
    /// Adds a file's cards: a CSV export from another app, or a collection file written
    /// by this app. Cards for a printing already owned add to its quantity.
    pub fn import(&self, data: &[u8], progress: &mut dyn FnMut(usize, usize)) -> Result<ImportResult> {
        self.commit_import(self.preview_import(data, progress)?)
    }

    /// Reads and matches a file's cards without saving anything, and says what
    /// importing them would do.
    pub fn preview_import(&self, data: &[u8], progress: &mut dyn FnMut(usize, usize)) -> Result<ImportPreview> {
        let (cards, report) = parse_import(self.game.as_ref(), data, progress)?;
        let (new_cards, already_owned) = self.store.count_new(&cards);
        let copies = cards.iter().map(Card::copy_count).sum();
        let price_paid = cards.iter().map(|c| c.price_paid().0).sum();
        Ok(ImportPreview { cards, report, copies, new_cards, already_owned, price_paid })
    }

    /// Saves the cards of a preview. The counts are worked out again, since the
    /// collection may have changed since the preview. Cards that came in without a
    /// price (such as non-English printings) are then priced, so they don't wait for
    /// the next refresh.
    pub fn commit_import(&self, preview: ImportPreview) -> Result<ImportResult> {
        let ImportPreview { cards, report, copies, .. } = preview;
        let unpriced: Vec<String> = cards.iter().filter(|c| c.market.is_empty()).map(|c| c.id.clone()).collect();
        let backup = self.store.backup("import")?;
        let (new_cards, already_owned) = self.store.add_cards(cards)?;
        if !unpriced.is_empty() {
            self.price_cards(&unpriced);
        }
        tracing::info!(
            game = self.game.id(),
            format = report.format,
            cards = copies,
            new = new_cards,
            merged = already_owned,
            "import complete"
        );
        Ok(ImportResult { report, cards: copies, new_cards, already_owned, backup })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_files() {
        assert!(is_app_file(b"\xef\xbb\xbf  {\"cards\":{}}"));
        assert!(!is_app_file(b"Name,Qty"));
        let r = ImportResult {
            report: ImportReport { format: APP_FILE_FORMAT.into(), rows: 2, notes: vec!["note".into()], ..Default::default() },
            cards: 3,
            new_cards: 1,
            already_owned: 1,
            ..Default::default()
        };
        assert_eq!(
            r.summary(),
            "Format: TCG Binder collection file\nCards read: 2\nCards imported: 3 (1 new cards, 1 added to cards you already own)\n\nnote\n"
        );
    }
}
