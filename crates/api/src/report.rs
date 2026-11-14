//! The valuation report: one self-contained HTML page listing every card by set with
//! its value, to open in a browser and print or save as PDF (for insurance, say).
//! Card text comes from the catalog and the user, so all of it goes through [`escape`].

use std::collections::BTreeMap;
use std::fmt::Write;

use tcgbinder_core::collection::{Card, Condition, compare_collector_number, round_cents};
use tcgbinder_core::dates;
use tcgbinder_core::service::GameService;

/// Makes text safe to put in HTML, in elements and in quoted attributes.
pub(crate) fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Dollars with thousands separators: "$1,234.50".
fn usd(v: f64) -> String {
    let cents = (round_cents(v) * 100.0).round() as i64;
    let (sign, cents) = if cents < 0 { ("-", -cents) } else { ("", cents) };
    let dollars = (cents / 100).to_string();
    let mut grouped = String::new();
    for (i, ch) in dollars.chars().enumerate() {
        if i > 0 && (dollars.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    format!("{sign}${grouped}.{:02}", cents % 100)
}

fn condition_name(c: Condition) -> &'static str {
    match c {
        Condition::NearMint => "Near Mint",
        Condition::LightPlay => "Lightly Played",
        Condition::Moderate => "Moderately Played",
        Condition::HeavyPlay => "Heavily Played",
        Condition::Damaged => "Damaged",
    }
}

const STYLE: &str = "
html { color-scheme: light; background: #fff; }
.wide { overflow-x: auto; }
body { font: 13px/1.4 -apple-system, system-ui, 'Segoe UI', sans-serif; color: #111; margin: 24px; }
h1 { font-size: 22px; margin: 0 0 4px; }
h2 { font-size: 16px; margin: 28px 0 6px; }
.muted { color: #555; }
.totals { display: flex; flex-wrap: wrap; gap: 8px 28px; margin: 14px 0; padding: 10px 0; border-top: 1px solid #ccc; border-bottom: 1px solid #ccc; }
.totals div b { display: block; font-size: 17px; }
table { width: 100%; border-collapse: collapse; }
th, td { text-align: left; padding: 3px 6px; border-bottom: 1px solid #e3e3e3; vertical-align: top; }
th { font-size: 12px; color: #555; border-bottom: 1px solid #999; }
td.n, th.n { text-align: right; font-variant-numeric: tabular-nums; white-space: nowrap; }
tfoot td { font-weight: 600; border-bottom: 0; border-top: 1px solid #999; }
tr { break-inside: avoid; }
@media print {
  body { margin: 0; }
  .wide { overflow: visible; }
  section.set + section.set { break-before: page; }
  thead { display: table-header-group; }
  tfoot { display: table-row-group; }
}
";

/// One line of the report: a card's copies in one finish and condition.
struct Line<'a> {
    card: &'a Card,
    finish: String,
    condition: Condition,
    quantity: i64,
    each: Option<f64>,
}

/// The report for `cards`. `filtered` says they are only some of the collection.
pub(crate) fn valuation(g: &GameService, cards: &[Card], filtered: bool) -> String {
    let finish_rank = |f: &str| g.game.finishes().iter().position(|x| *x == f).unwrap_or(usize::MAX);
    // Sets, newest first; in each, cards by collector number.
    let mut sets: BTreeMap<(std::cmp::Reverse<String>, String), Vec<&Card>> = BTreeMap::new();
    for c in cards {
        sets.entry((std::cmp::Reverse(c.released_at.clone()), c.set.to_uppercase())).or_default().push(c);
    }
    let copies: i64 = cards.iter().map(Card::copy_count).sum();
    let market: f64 = cards.iter().map(|c| c.market_value().0).sum();
    let (paid, paid_unknown) = cards.iter().map(Card::price_paid).fold((0.0, 0), |(a, b), (x, y)| (a + x, b + y));
    let unpriced: i64 = cards.iter().map(|c| c.market_value().1).sum();
    let prices_as_of = cards.iter().map(|c| c.prices_updated.as_str()).max().unwrap_or("");

    let mut out = String::new();
    let title = format!("{} collection: valuation", g.game.name());
    let _ = writeln!(
        out,
        "<!DOCTYPE html>\n<html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <title>{t}</title><style>{STYLE}</style></head><body>\n<h1>{t}</h1>\n<p class=\"muted\">Made {today} by TCG Binder. Market prices as of {asof}.{scope}</p>",
        t = escape(&title),
        today = dates::today_string(),
        asof = if prices_as_of.is_empty() { "never".to_string() } else { escape(prices_as_of) },
        scope = if filtered { " This report covers only the cards that matched the collection list's filters." } else { "" },
    );
    let _ = writeln!(
        out,
        "<div class=\"totals\"><div>Cards<b>{copies}</b></div><div>Unique cards<b>{unique}</b></div><div>Sets<b>{nsets}</b></div>\
         <div>Price paid<b>{paid}</b></div><div>Market value<b>{market}</b></div></div>",
        unique = cards.len(),
        nsets = sets.len(),
        paid = usd(paid),
        market = usd(market),
    );
    let mut notes = Vec::new();
    if paid_unknown > 0 {
        notes.push(format!("{paid_unknown} cards have no price paid."));
    }
    if unpriced > 0 {
        notes.push(format!("{unpriced} cards have no market price and count as $0.00."));
    }
    if !notes.is_empty() {
        let _ = writeln!(out, "<p class=\"muted\">{}</p>", notes.join(" "));
    }
    if cards.is_empty() {
        out.push_str("<p>No cards.</p>\n");
    }

    for ((_, code), mut set_cards) in sets {
        set_cards.sort_by(|a, b| compare_collector_number(&a.collector_number, &b.collector_number).then_with(|| a.name.cmp(&b.name)));
        let set_name = &set_cards[0].set_name;
        let mut lines = Vec::new();
        for c in &set_cards {
            let mut by: BTreeMap<(usize, String, Condition), i64> = BTreeMap::new();
            for (finish, cond, p) in c.copies.each() {
                *by.entry((finish_rank(finish), finish.to_string(), cond)).or_default() += p.quantity;
            }
            for ((_, finish, condition), quantity) in by.into_iter().filter(|(_, q)| *q > 0) {
                lines.push(Line { card: c, each: c.market_price(&finish), finish, condition, quantity });
            }
        }
        let set_value: f64 = set_cards.iter().map(|c| c.market_value().0).sum();
        let set_copies: i64 = set_cards.iter().map(|c| c.copy_count()).sum();
        let heading = if set_name.is_empty() {
            escape(&code)
        } else {
            format!("{} <span class=\"muted\">{}</span>", escape(set_name), escape(&code))
        };
        let _ = writeln!(
            out,
            "<section class=\"set\"><h2>{heading}</h2>\n<div class=\"wide\"><table><thead><tr><th>Card</th><th>No.</th><th>Finish</th><th>Condition</th>\
             <th class=\"n\">Qty</th><th class=\"n\">Each</th><th class=\"n\">Value</th></tr></thead><tbody>"
        );
        for l in &lines {
            let lang = if l.card.language.is_empty() || l.card.language == "en" {
                String::new()
            } else {
                format!(" ({})", escape(&l.card.language))
            };
            let _ = writeln!(
                out,
                "<tr><td>{name}{lang}</td><td>{num}</td><td>{finish}</td><td>{cond}</td><td class=\"n\">{q}</td><td class=\"n\">{each}</td><td class=\"n\">{value}</td></tr>",
                name = escape(&l.card.name),
                num = escape(&l.card.collector_number),
                finish = escape(&g.game.finish_name(&l.finish)),
                cond = condition_name(l.condition),
                q = l.quantity,
                each = l.each.map_or("—".to_string(), usd),
                value = l.each.map_or("—".to_string(), |e| usd(e * l.quantity as f64)),
            );
        }
        let _ = writeln!(
            out,
            "</tbody><tfoot><tr><td colspan=\"4\">{} total</td><td class=\"n\">{set_copies}</td><td></td><td class=\"n\">{}</td></tr></tfoot></table></div></section>",
            escape(&code),
            usd(set_value)
        );
    }
    out.push_str("</body></html>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_and_money() {
        assert_eq!(escape(r#"<script>alert("x")</script> & 'y'"#), "&lt;script&gt;alert(&quot;x&quot;)&lt;/script&gt; &amp; &#39;y&#39;");
        assert_eq!(usd(1234567.5), "$1,234,567.50");
        assert_eq!(usd(0.0), "$0.00");
        assert_eq!(usd(-12.345), "-$12.35");
        assert_eq!(usd(999.999), "$1,000.00");
    }
}
