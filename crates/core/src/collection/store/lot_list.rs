//! Putting copies you already own into a lot from a card list (a product's contents,
//! say): each line is matched to copies in no lot, and those are moved into it. Copies
//! already in a lot are never taken, so one list can't pull cards out of another lot.

use std::collections::HashMap;

use anyhow::Result;
use serde::Serialize;

use super::{Store, StoreData};
use crate::collection::tags::{LOT_PREFIX, normalize_tags};
use crate::collection::{Card, Copies, Purchase};

/// One line of a card list, as the game reads it ([`crate::game::Game::parse_list`]).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ListLine {
    /// The line's number in the list, from 1.
    pub line: u64,
    /// The line as written.
    pub text: String,
    pub name: String,
    /// The set code, lowercase; empty when the line doesn't say.
    pub set: String,
    /// The collector number; empty when the line doesn't say.
    pub number: String,
    pub quantity: i64,
    pub finish: String,
}

/// Copies of one printing a line matched.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ListFound {
    pub key: String,
    pub name: String,
    pub set: String,
    pub number: String,
    pub finish: String,
    pub quantity: i64,
}

/// What one line of the list matched.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ListMatch {
    pub line: u64,
    pub text: String,
    pub name: String,
    pub wanted: i64,
    /// Copies in no lot that the line takes, by printing.
    pub found: Vec<ListFound>,
    /// Copies matching the line that are in a lot already: left where they are.
    pub in_lots: i64,
    /// The copies the collection is short, as a list line ("2 Shock (M19) 156"), to add
    /// as new cards; empty when every copy was found.
    pub missing: String,
}

impl ListMatch {
    pub fn found_count(&self) -> i64 {
        self.found.iter().map(|f| f.quantity).sum()
    }
}

/// Whether a purchase is in a lot.
fn in_a_lot(p: &Purchase) -> bool {
    p.tags.iter().any(|t| t.to_lowercase().starts_with(LOT_PREFIX))
}

/// Whether the card is the one a line names: its name, or one face of it.
fn named(c: &Card, want: &str) -> bool {
    let name = c.name.to_lowercase();
    name == want || name.split(" // ").any(|face| face == want)
}

/// Matches each line to copies in no lot. A line with a set (and number) takes that
/// printing only. A line with a name alone prefers printings released by `date` (the
/// lot's date), newest first, then later ones, oldest first. Copies are only counted
/// once across lines.
fn plan(data: &StoreData, lines: &[ListLine], date: &str) -> Vec<ListMatch> {
    // Copies by (card key, finish): in no lot, and in one.
    let (mut free, mut held) = (HashMap::<(&str, String), i64>::new(), HashMap::<(&str, String), i64>::new());
    for (key, c) in &data.cards {
        for (finish, _, p) in c.copies.each() {
            let into = if in_a_lot(p) { &mut held } else { &mut free };
            *into.entry((key.as_str(), finish.to_lowercase())).or_default() += p.quantity;
        }
    }
    lines
        .iter()
        .map(|l| {
            let want = l.name.trim().to_lowercase();
            let finish = l.finish.to_lowercase();
            let mut cands: Vec<(&String, &Card)> = data
                .cards
                .iter()
                .filter(|(_, c)| named(c, &want))
                .filter(|(_, c)| l.set.is_empty() || c.set.eq_ignore_ascii_case(&l.set))
                .filter(|(_, c)| l.number.is_empty() || c.collector_number.eq_ignore_ascii_case(&l.number))
                .collect();
            let by_date = |c: &Card| !date.is_empty() && c.released_at.as_str() > date;
            cands.sort_by(|(_, a), (_, b)| {
                by_date(a)
                    .cmp(&by_date(b))
                    .then_with(|| if by_date(a) { a.released_at.cmp(&b.released_at) } else { b.released_at.cmp(&a.released_at) })
            });
            let mut m = ListMatch { line: l.line, text: l.text.clone(), name: l.name.clone(), wanted: l.quantity, ..ListMatch::default() };
            let mut need = l.quantity;
            for (key, c) in cands {
                let k = (key.as_str(), finish.clone());
                m.in_lots += held.get(&k).copied().unwrap_or(0);
                let have = free.entry(k).or_default();
                let take = need.min(*have);
                if take > 0 {
                    *have -= take;
                    need -= take;
                    let (set, number) = (c.set.clone(), c.collector_number.clone());
                    m.found.push(ListFound { key: key.clone(), name: c.name.clone(), set, number, finish: finish.clone(), quantity: take });
                }
            }
            if need > 0 {
                m.missing = missing_line(l, need);
            }
            m
        })
        .collect()
}

/// A list line for `quantity` copies of what `l` names.
fn missing_line(l: &ListLine, quantity: i64) -> String {
    let mut s = format!("{quantity} {}", l.name);
    if !l.set.is_empty() {
        s += &format!(" ({})", l.set.to_uppercase());
        if !l.number.is_empty() {
            s += &format!(" {}", l.number);
        }
    }
    match l.finish.as_str() {
        "foil" => s += " *F*",
        "etched" => s += " *E*",
        _ => {}
    }
    s
}

impl Store {
    /// What putting `lines` into a lot would take (see [`plan`]); changes nothing.
    pub fn match_list(&self, lines: &[ListLine], date: &str) -> Vec<ListMatch> {
        self.file.read(|d| plan(d, lines, date))
    }

    /// Puts the copies `lines` match into the lot with `tag`, in one save: only copies
    /// in no lot, matched again as the file stands. `edit` is applied to each purchase
    /// moved (the lot sets its date). Returns the matches and the copies moved.
    pub fn put_list_in_lot(
        &self,
        tag: &str,
        lines: &[ListLine],
        date: &str,
        edit: impl Fn(&mut Purchase),
    ) -> Result<(Vec<ListMatch>, i64)> {
        self.file.mutate(|data| {
            let matches = plan(data, lines, date);
            let mut moved = 0;
            for f in matches.iter().flat_map(|m| &m.found) {
                let Some(card) = data.cards.get_mut(&f.key) else { continue };
                let mut want = f.quantity;
                let mut copies = Copies::default();
                for r in card.purchase_rows() {
                    let p = r.purchase;
                    let take = if want > 0 && r.finish.eq_ignore_ascii_case(&f.finish) && !in_a_lot(&p) { want.min(p.quantity) } else { 0 };
                    if take > 0 {
                        let mut tags = p.tags.clone();
                        tags.push(tag.to_string());
                        let mut joined = Purchase { quantity: take, tags: normalize_tags(&tags)?, ..p.clone() };
                        edit(&mut joined);
                        copies.add(&r.finish, r.condition, joined);
                        want -= take;
                        moved += take;
                    }
                    copies.add(&r.finish, r.condition, Purchase { quantity: p.quantity - take, ..p });
                }
                card.copies = copies;
            }
            Ok((matches, moved))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::Condition;

    fn card(key: &str, name: &str, set: &str, released: &str, finish: &str, quantity: i64, tags: &[&str]) -> (String, Card) {
        let mut copies = Copies::default();
        let tags = tags.iter().map(|t| t.to_string()).collect();
        copies.add(finish, Condition::NearMint, Purchase { quantity, tags, ..Purchase::default() });
        let c = Card { id: key.into(), name: name.into(), set: set.into(), released_at: released.into(), copies, ..Card::default() };
        (key.to_string(), c)
    }

    fn line(text: &str, name: &str, set: &str, quantity: i64) -> ListLine {
        ListLine {
            line: 1,
            text: text.into(),
            name: name.into(),
            set: set.into(),
            quantity,
            finish: "nonfoil".into(),
            ..ListLine::default()
        }
    }

    #[test]
    fn a_name_takes_printings_from_before_the_lot_first() {
        let data = StoreData {
            cards: [
                card("old", "Shock", "m11", "2010-07-16", "nonfoil", 1, &[]),
                card("m19", "Shock", "m19", "2018-07-13", "nonfoil", 1, &[]),
                card("new", "Shock", "m21", "2020-07-03", "nonfoil", 5, &[]),
                card("held", "Shock", "dom", "2018-04-27", "nonfoil", 3, &["lot:x"]),
            ]
            .into(),
            ..StoreData::default()
        };
        let keys = |m: &ListMatch| m.found.iter().map(|f| (f.key.clone(), f.quantity)).collect::<Vec<_>>();
        // Bought in August 2018: M19, then M11, then the later M21; never the lot's DOM.
        let m = &plan(&data, &[line("4 Shock", "Shock", "", 4)], "2018-08-01")[0];
        assert_eq!(keys(m), [("m19".to_string(), 1), ("old".to_string(), 1), ("new".to_string(), 2)]);
        assert_eq!((m.in_lots, m.missing.as_str()), (3, ""));
        // A set code keeps to that printing; two lines don't count the same copy twice.
        let ms = plan(&data, &[line("2 Shock (M19)", "Shock", "m19", 2), line("1 Shock (M19)", "Shock", "m19", 1)], "");
        assert_eq!((keys(&ms[0]), ms[0].missing.as_str()), (vec![("m19".to_string(), 1)], "1 Shock (M19)"));
        assert_eq!((keys(&ms[1]), ms[1].missing.as_str()), (vec![], "1 Shock (M19)"));
    }

    #[test]
    fn a_face_names_a_split_card() {
        let data = StoreData { cards: [card("fi", "Fire // Ice", "mh2", "2021-06-18", "nonfoil", 1, &[])].into(), ..StoreData::default() };
        assert_eq!(plan(&data, &[line("1 Fire", "Fire", "", 1)], "")[0].found_count(), 1);
    }
}
