//! The collection as CSV, one row per purchase, for spreadsheets and other apps. The
//! headers are ones the importer knows, so the app reads its own CSV back. Tags are
//! joined with "; " in one column.

use anyhow::Result;

use super::Card;

/// The columns before the game's catalog ID column.
pub const CSV_COLUMNS: &[&str] = &[
    "Name",
    "Set Code",
    "Set Name",
    "Collector Number",
    "Language",
    "Finish",
    "Condition",
    "Quantity",
    "Price Paid",
    "Acquired Date",
    "Note",
    "Tags",
    "Rarity",
    "Market Price",
];

/// A price as the CSV shows it: blank when there is none.
fn money(v: Option<f64>) -> String {
    v.filter(|v| *v > 0.0).map(|v| format!("{}", (v * 10_000.0).round() / 10_000.0)).unwrap_or_default()
}

/// Writes `cards` as CSV in their order, a row for each purchase. `id_label` heads
/// the last column, the catalog's ID (e.g. "Scryfall ID").
pub fn write_csv(cards: &[Card], id_label: &str) -> Result<Vec<u8>> {
    let mut w = csv::Writer::from_writer(Vec::new());
    w.write_record(CSV_COLUMNS.iter().copied().chain([id_label]))?;
    for c in cards {
        for r in c.purchase_rows() {
            let p = &r.purchase;
            w.write_record([
                c.name.as_str(),
                &c.set.to_uppercase(),
                &c.set_name,
                &c.collector_number,
                &c.language,
                &r.finish,
                r.condition.code(),
                &p.quantity.to_string(),
                &money(Some(p.price_paid)),
                &p.acquired_date,
                &p.note,
                &p.tags.join("; "),
                &c.rarity,
                &money(c.market_price(&r.finish)),
                &c.id,
            ])?;
        }
    }
    Ok(w.into_inner()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::{Condition, Purchase};

    #[test]
    fn a_row_per_purchase() {
        let mut c = Card {
            id: "abc".into(),
            name: "Fire // Ice".into(),
            set: "mh2".into(),
            set_name: "Modern Horizons 2".into(),
            collector_number: "290".into(),
            language: "en".into(),
            rarity: "uncommon".into(),
            ..Card::default()
        };
        c.market.insert("foil".into(), 1.5);
        c.add_purchase(
            "nonfoil",
            Condition::NearMint,
            Purchase { quantity: 2, price_paid: 0.1234, acquired_date: "2024-03-15".into(), ..Default::default() },
        );
        c.add_purchase(
            "foil",
            Condition::LightPlay,
            Purchase {
                quantity: 1,
                note: "trade, \"binder\"".into(),
                tags: vec!["binder A".into(), "for sale".into()],
                ..Default::default()
            },
        );
        let out = String::from_utf8(write_csv(&[c], "Scryfall ID").unwrap()).unwrap();
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(
            lines[0],
            "Name,Set Code,Set Name,Collector Number,Language,Finish,Condition,Quantity,Price Paid,Acquired Date,Note,Tags,Rarity,Market Price,Scryfall ID"
        );
        assert!(lines.contains(&"Fire // Ice,MH2,Modern Horizons 2,290,en,nonfoil,NM,2,0.1234,2024-03-15,,,uncommon,,abc"));
        assert!(lines.contains(
            &"Fire // Ice,MH2,Modern Horizons 2,290,en,foil,LP,1,,,\"trade, \"\"binder\"\"\",binder A; for sale,uncommon,1.5,abc"
        ));
        assert_eq!(lines.len(), 3);
    }
}
