//! Lots: cards bought together for one price (a 500-card lot, a bundle). The cards
//! added under a lot point to it (`Purchase::lot`), and the lot's total is split
//! among them: evenly, or by each card's market value. The lot record lives in
//! `lots.json` beside the collection (a [`Records`](crate::records::Records) file); its cards' prices are worked out here, as a
//! pure function, and saved to the collection by `Store::reprice_lot`.

use std::collections::{BTreeMap, HashMap};

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::collection::Condition;
use crate::records::{Record, new_id};

/// How many cards a lot holds, which the even split divides by.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LotCount {
    /// The owner counted the lot.
    Known { cards: i64 },
    /// A bundle: packs × cards per pack, plus loose cards and basic lands.
    Bundle {
        packs: i64,
        cards_per_pack: i64,
        #[serde(default)]
        extra_cards: i64,
        #[serde(default)]
        lands: i64,
        /// Whether the basic lands take a share of the cost (even split only).
        #[serde(default)]
        lands_share_cost: bool,
    },
    /// Not counted: divide by what gets entered.
    #[default]
    AsEntered,
}

/// How a lot's total is split among its cards.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LotSplit {
    /// In proportion to each copy's market price.
    #[default]
    ByValue,
    /// The same price for every card.
    Even,
}

/// Cards bought together for one price.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Lot {
    pub id: String,
    /// "Bloomburrow bundle", "500-card lot from eBay".
    pub name: String,
    /// `YYYY-MM-DD`; every card in the lot gets it.
    pub acquired_date: String,
    pub total_paid: f64,
    pub count: LotCount,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub note: String,
    pub split: LotSplit,
    /// Still being entered. While open, the cards carry an even-split price; a value
    /// split is worked out when the lot is finished.
    pub open: bool,
    /// When the lot was made (an RFC 3339 time).
    pub created: String,
    /// The day a value split was last worked out, from that day's prices.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub split_date: String,
}

/// One purchase's copies in a lot, as the split sees them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LotCopy {
    pub card_id: String,
    pub finish: String,
    pub condition: Condition,
    pub quantity: i64,
    /// The market price of one copy in its finish.
    pub market: Option<f64>,
    pub rarity: String,
    /// The game calls it a basic land (for bundles whose lands don't share the cost).
    pub basic_land: bool,
}

/// A group of a lot's copies: one card in one finish and condition.
pub type CopyKey = (String, String, Condition);

/// The prices a lot gives its cards.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LotPrices {
    /// The price of each copy, per card, finish and condition.
    pub each: BTreeMap<CopyKey, f64>,
    /// One copy of this group costs this much more (or less), so the lot adds up to
    /// its total exactly (a value split's rounding remainder).
    pub extra: Option<(CopyKey, f64)>,
    /// The price of one card under an even split (what an even split, or an open
    /// lot, charges).
    pub even: Option<f64>,
    /// Said to the owner, e.g. that a value split fell back to an even one.
    pub note: String,
}

/// A price kept to four decimal places, so 500 cards for $38 cost $0.0760 each and add
/// back up to the total.
pub fn round4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

impl Lot {
    /// Whether a copy counts toward the even split's divisor: a bundle's basic lands
    /// don't, unless they share the cost.
    fn counts(&self, c: &LotCopy) -> bool {
        !matches!(self.count, LotCount::Bundle { lands_share_cost: false, .. }) || !c.basic_land
    }

    /// The copies entered so far that count toward the divisor.
    pub fn entered(&self, copies: &[LotCopy]) -> i64 {
        copies.iter().filter(|c| self.counts(c)).map(|c| c.quantity).sum()
    }

    /// The number of cards the lot was counted as holding (the even split's divisor);
    /// `None` when it is counted as entered.
    pub fn expected(&self) -> Option<i64> {
        match self.count {
            LotCount::Known { cards } => Some(cards),
            LotCount::Bundle { packs, cards_per_pack, extra_cards, lands, lands_share_cost } => {
                Some(packs * cards_per_pack + extra_cards + if lands_share_cost { lands } else { 0 })
            }
            LotCount::AsEntered => None,
        }
    }

    /// The price of one card under an even split, given what has been entered.
    pub fn even_price(&self, copies: &[LotCopy]) -> Option<f64> {
        let divisor = self.expected().unwrap_or_else(|| self.entered(copies));
        (divisor > 0).then(|| round4(self.total_paid / divisor as f64))
    }

    /// Checks a lot's fields.
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            bail!("give the lot a name");
        }
        if !self.total_paid.is_finite() || self.total_paid < 0.0 {
            bail!("the total paid can't be negative");
        }
        let bad = match self.count {
            LotCount::Known { cards } => cards < 1,
            LotCount::Bundle { packs, cards_per_pack, extra_cards, lands, .. } => {
                packs < 0 || cards_per_pack < 0 || extra_cards < 0 || lands < 0 || packs * cards_per_pack + extra_cards + lands < 1
            }
            LotCount::AsEntered => false,
        };
        if bad {
            bail!("the lot must hold at least one card");
        }
        Ok(())
    }
}

/// The prices of a lot's copies. While the lot is open, or for an even split, every
/// copy costs [`Lot::even_price`] (a bundle's basic lands nothing, unless they share
/// the cost). A finished value-split lot divides its total in proportion to each
/// copy's market price; a copy without one is valued at the average of the lot's
/// priced copies of its rarity (else the lowest such average), and a lot with no
/// prices at all falls back to an even split. Shares are kept to four decimal places,
/// with the rounding remainder on one copy of the most valuable card, so the lot adds
/// up to its total exactly.
pub fn lot_prices(lot: &Lot, copies: &[LotCopy]) -> LotPrices {
    let even = lot.even_price(copies);
    let mut out = LotPrices { even, ..LotPrices::default() };
    let key = |c: &LotCopy| (c.card_id.clone(), c.finish.clone(), c.condition);
    if lot.open || lot.split == LotSplit::Even || copies.is_empty() {
        for c in copies {
            out.each.insert(key(c), if lot.counts(c) { even.unwrap_or(0.0) } else { 0.0 });
        }
        return out;
    }

    // Each copy's value: its market price, or an estimate from its rarity.
    let mut by_rarity: HashMap<&str, (f64, i64)> = HashMap::new();
    for c in copies {
        if let Some(p) = c.market {
            let e = by_rarity.entry(c.rarity.as_str()).or_default();
            e.0 += p * c.quantity as f64;
            e.1 += c.quantity;
        }
    }
    let average = |r: &str| by_rarity.get(r).filter(|(_, n)| *n > 0).map(|(sum, n)| sum / *n as f64);
    let lowest = by_rarity.values().filter(|(_, n)| *n > 0).map(|(sum, n)| sum / *n as f64).reduce(f64::min);
    let value = |c: &LotCopy| c.market.or_else(|| average(&c.rarity)).or(lowest).unwrap_or(0.0);
    let total_value: f64 = copies.iter().map(|c| value(c) * c.quantity as f64).sum();
    if total_value <= 0.0 {
        let fallback = Lot { split: LotSplit::Even, ..lot.clone() };
        let mut out = lot_prices(&fallback, copies);
        out.note = "None of the lot's cards has a market price, so its total is split evenly.".into();
        return out;
    }
    let mut sum = 0.0;
    for c in copies {
        let each = round4(lot.total_paid * value(c) / total_value);
        sum += each * c.quantity as f64;
        out.each.insert(key(c), each);
    }
    let rest = round4(lot.total_paid - sum);
    if rest != 0.0
        && let Some(top) = copies.iter().max_by(|a, b| value(a).total_cmp(&value(b)))
    {
        out.extra = Some((key(top), rest));
    }
    out
}

impl Record for Lot {
    const FILE: &'static str = "lots.json";
    const NAME: &'static str = "lot";
    fn id(&self) -> String {
        self.id.clone()
    }
    /// A new lot gets an ID and a creation time, and starts open, with no split worked
    /// out.
    fn assign_id(&mut self) {
        self.id = new_id();
        self.created = chrono::Local::now().to_rfc3339();
        self.open = true;
        self.split_date.clear();
    }
    fn check(&mut self) -> Result<()> {
        self.validate()
    }
    /// Newest first.
    fn order(a: &Self, b: &Self) -> std::cmp::Ordering {
        b.created.cmp(&a.created)
    }
    /// Where both have a lot, the one created later wins.
    fn merge(&mut self, theirs: Self) {
        if theirs.created > self.created {
            *self = theirs;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::records::Records;

    fn copy(id: &str, finish: &str, q: i64, market: Option<f64>, rarity: &str) -> LotCopy {
        LotCopy {
            card_id: id.into(),
            finish: finish.into(),
            condition: Condition::NearMint,
            quantity: q,
            market,
            rarity: rarity.into(),
            basic_land: false,
        }
    }

    fn lot(total: f64, count: LotCount, split: LotSplit, open: bool) -> Lot {
        Lot { name: "lot".into(), total_paid: total, count, split, open, ..Lot::default() }
    }

    /// What the lot's copies cost in all.
    fn sum(p: &LotPrices, copies: &[LotCopy]) -> f64 {
        let base: f64 = copies.iter().map(|c| p.each[&(c.card_id.clone(), c.finish.clone(), c.condition)] * c.quantity as f64).sum();
        round4(base + p.extra.as_ref().map_or(0.0, |(_, x)| *x))
    }

    fn price(p: &LotPrices, id: &str, finish: &str) -> f64 {
        p.each[&(id.to_string(), finish.to_string(), Condition::NearMint)]
    }

    #[test]
    fn value_split_is_proportional_and_exact() {
        let copies = [
            copy("a", "nonfoil", 1, Some(30.0), "rare"),
            copy("b", "nonfoil", 3, Some(0.05), "common"),
            copy("c", "nonfoil", 2, Some(24.925), "mythic"),
        ];
        let l = lot(55.0, LotCount::AsEntered, LotSplit::ByValue, false);
        let p = lot_prices(&l, &copies);
        assert_eq!(price(&p, "a", "nonfoil"), 20.625, "a $30 card in a lot worth $80 for $55");
        assert_eq!(price(&p, "b", "nonfoil"), 0.0344);
        assert_eq!(sum(&p, &copies), 55.0);
        let odd = lot(10.0, LotCount::AsEntered, LotSplit::ByValue, false);
        let three = [copy("x", "nonfoil", 3, Some(1.0), "common")];
        let p = lot_prices(&odd, &three);
        assert_eq!(price(&p, "x", "nonfoil"), 3.3333);
        assert_eq!(p.extra, Some((("x".into(), "nonfoil".into(), Condition::NearMint), 0.0001)));
        assert_eq!(sum(&p, &three), 10.0, "the remainder lands on one copy");
    }

    #[test]
    fn value_split_estimates_and_falls_back() {
        // An unpriced common takes the commons' average; an unpriced uncommon (no priced
        // uncommons) the lowest average in the lot.
        let copies = [
            copy("c1", "nonfoil", 1, Some(0.10), "common"),
            copy("c2", "nonfoil", 1, Some(0.30), "common"),
            copy("c3", "nonfoil", 1, None, "common"),
            copy("r", "nonfoil", 1, Some(1.40), "rare"),
            copy("u", "nonfoil", 1, None, "uncommon"),
        ];
        let p = lot_prices(&lot(2.2, LotCount::AsEntered, LotSplit::ByValue, false), &copies);
        assert_eq!(price(&p, "c3", "nonfoil"), price(&p, "u", "nonfoil"));
        assert_eq!(price(&p, "c3", "nonfoil"), 0.2);
        assert_eq!(sum(&p, &copies), 2.2);

        let unpriced = [copy("a", "nonfoil", 2, None, "common"), copy("b", "nonfoil", 2, None, "rare")];
        let p = lot_prices(&lot(4.0, LotCount::AsEntered, LotSplit::ByValue, false), &unpriced);
        assert_eq!(price(&p, "a", "nonfoil"), 1.0);
        assert!(p.note.contains("split evenly"));
    }

    #[test]
    fn foil_and_nonfoil_copies_differ() {
        let copies = [copy("a", "nonfoil", 1, Some(1.0), "rare"), copy("a", "foil", 1, Some(3.0), "rare")];
        let p = lot_prices(&lot(2.0, LotCount::AsEntered, LotSplit::ByValue, false), &copies);
        assert_eq!((price(&p, "a", "nonfoil"), price(&p, "a", "foil")), (0.5, 1.5));
    }

    #[test]
    fn open_lots_and_even_splits() {
        let copies = [copy("a", "nonfoil", 2, Some(10.0), "rare"), copy("b", "nonfoil", 1, Some(0.1), "common")];
        // Open: provisional even split, whatever the split.
        let p = lot_prices(&lot(38.0, LotCount::Known { cards: 500 }, LotSplit::ByValue, true), &copies);
        assert_eq!((price(&p, "a", "nonfoil"), p.even), (0.076, Some(0.076)));
        assert!(p.extra.is_none());
        // Five hundred cards for $38 add back up to the total within a cent.
        let many: Vec<LotCopy> = (0..500).map(|i| copy(&format!("c{i}"), "nonfoil", 1, Some(0.1), "common")).collect();
        let p = lot_prices(&lot(38.0, LotCount::Known { cards: 500 }, LotSplit::Even, false), &many);
        assert!((sum(&p, &many) - 38.0).abs() < 0.01);
        // As entered: earlier cards are repriced as more arrive.
        let as_entered = lot(9.0, LotCount::AsEntered, LotSplit::Even, true);
        assert_eq!(price(&lot_prices(&as_entered, &copies[..1]), "a", "nonfoil"), 4.5);
        assert_eq!(price(&lot_prices(&as_entered, &copies), "a", "nonfoil"), 3.0);
    }

    #[test]
    fn bundle_divisors_and_lands() {
        let bundle = |share| LotCount::Bundle { packs: 9, cards_per_pack: 14, extra_cards: 1, lands: 40, lands_share_cost: share };
        let mut land = copy("forest", "nonfoil", 4, Some(0.05), "common");
        land.basic_land = true;
        let copies = [copy("a", "nonfoil", 2, Some(1.0), "rare"), land];
        let off = lot(49.99, bundle(false), LotSplit::Even, false);
        assert_eq!(off.expected(), Some(127));
        assert_eq!(off.entered(&copies), 2, "lands don't count when they don't share the cost");
        let p = lot_prices(&off, &copies);
        assert_eq!((price(&p, "a", "nonfoil"), price(&p, "forest", "nonfoil")), (0.3936, 0.0));
        let on = lot(49.99, bundle(true), LotSplit::Even, false);
        assert_eq!((on.expected(), on.entered(&copies)), (Some(167), 6));
        assert_eq!(price(&lot_prices(&on, &copies), "forest", "nonfoil"), 0.2993);
        assert!(lot(1.0, LotCount::Known { cards: 0 }, LotSplit::Even, true).validate().is_err());
    }

    #[test]
    fn store_and_conflict_merge() {
        let dir = tempfile::tempdir().unwrap();
        let s: Records<Lot> = Records::open(dir.path()).unwrap();
        let a = s.add(lot(10.0, LotCount::AsEntered, LotSplit::Even, false)).unwrap();
        assert!(a.open && a.id.len() == 16 && !a.created.is_empty());
        assert!(s.add(Lot { name: " ".into(), ..Lot::default() }).is_err());
        let mut renamed = a.clone();
        renamed.name = "renamed".into();
        s.save(renamed).unwrap();
        assert_eq!(Records::<Lot>::open(dir.path()).unwrap().get(&a.id).unwrap().name, "renamed");

        let mut later = a.clone();
        later.name = "theirs".into();
        later.created = "9999".into();
        let other = Lot { id: "other".into(), name: "other".into(), created: "1".into(), ..Lot::default() };
        let mut older = a.clone();
        older.name = "old".into();
        older.created = "0".into();
        assert_eq!(s.merge_copy(vec![later, other]).unwrap(), 2);
        assert_eq!(s.get(&a.id).unwrap().name, "theirs");
        assert_eq!(s.merge_copy(vec![older]).unwrap(), 0);
        assert_eq!(s.all().len(), 2);
        s.remove("other").unwrap();
        assert_eq!(s.all().len(), 1);
    }
}
