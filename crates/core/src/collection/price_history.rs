use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};

use anyhow::{Context, Result, anyhow, bail};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use super::{Card, TimePoint};
use crate::dates;

/// Daily market prices per printing and finish, so the collection's real value can be
/// charted over time. It is filled by downloading price history (up to 90 days back,
/// for Magic) and by recording the catalog's prices each day the app is used.
pub struct PriceHistory {
    path: PathBuf,
    data: RwLock<HistoryData>,
    /// Serializes saves so writes land in order. Holds the prices on disk: a save never
    /// writes fewer.
    saved_points: Mutex<usize>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct HistoryData {
    /// The date price history was last loaded.
    #[serde(skip_serializing_if = "String::is_empty")]
    history_loaded: String,
    /// Catalog ID → finish → that printing's daily prices.
    cards: BTreeMap<String, BTreeMap<String, DailyPrices>>,
}

/// One printing's prices in one finish, one per day: `cents[i]` is the USD price in
/// cents on `from` + i days, or -1 for a day without a price. Prices are recorded for
/// nearly every day, so an array is far smaller and faster than a map by date, and
/// whole cents (which prices always are) encode much faster than decimals.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DailyPrices {
    /// `YYYY-MM-DD` of `cents[0]`.
    pub from: String,
    pub cents: Vec<i32>,
    /// `from` as a day number.
    #[serde(skip)]
    start: i64,
}

/// Marks a day without a recorded price.
const NO_PRICE: i32 = -1;

fn to_cents(usd: f64) -> i32 {
    (usd * 100.0).round() as i32
}

fn dollars(cents: i32) -> f64 {
    f64::from(cents) / 100.0
}

impl DailyPrices {
    /// Makes the series cover days `lo..=hi`, filling new days with no price.
    fn grow(&mut self, lo: i64, hi: i64) {
        if self.cents.is_empty() {
            self.start = lo;
        }
        if lo < self.start {
            let mut front = vec![NO_PRICE; (self.start - lo) as usize];
            front.append(&mut self.cents);
            self.cents = front;
            self.start = lo;
        }
        let end = self.start + self.cents.len() as i64 - 1;
        if end < hi {
            self.cents.resize(self.cents.len() + (hi - end) as usize, NO_PRICE);
        }
        self.from = dates::format(dates::from_day(self.start));
    }

    /// The price on day `d`, if one was recorded.
    fn at(&self, d: i64) -> Option<f64> {
        let i = usize::try_from(d - self.start).ok()?;
        self.cents.get(i).filter(|c| **c >= 0).map(|c| dollars(*c))
    }

    /// The indexes of the first and last recorded prices.
    fn recorded(&self) -> Option<(usize, usize)> {
        let first = self.cents.iter().position(|c| *c >= 0)?;
        let last = self.cents.iter().rposition(|c| *c >= 0)?;
        Some((first, last))
    }

    fn points(&self) -> usize {
        self.cents.iter().filter(|c| **c >= 0).count()
    }
}

impl HistoryData {
    fn points(&self) -> usize {
        self.cards.values().flat_map(BTreeMap::values).map(DailyPrices::points).sum()
    }

    /// The prices for a printing and finish, created if needed.
    fn series(&mut self, id: &str, finish: &str) -> &mut DailyPrices {
        self.cards.entry(id.to_string()).or_default().entry(finish.to_string()).or_default()
    }

    fn set(&mut self, id: &str, finish: &str, date: &str, price: f64) {
        let Some(d) = dates::day_number(date) else { return };
        if price < 0.0 {
            return;
        }
        let dp = self.series(id, finish);
        dp.grow(d, d);
        dp.cents[(d - dp.start) as usize] = to_cents(price);
    }
}

impl PriceHistory {
    /// Reads the history file at `path`; a missing file gives an empty history. A file
    /// that can't be parsed is never overwritten: it is renamed to
    /// `<path>.unreadable-<timestamp>` so it can be recovered, and a fresh history is
    /// returned with a warning saying so.
    pub fn open(path: impl Into<PathBuf>) -> Result<(PriceHistory, Option<String>)> {
        let path = path.into();
        let fresh = |path: PathBuf| PriceHistory { path, data: RwLock::default(), saved_points: Mutex::new(0) };
        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((fresh(path), None)),
            Err(e) => return Err(e.into()),
        };
        let mut data: HistoryData = match serde_json::from_slice(&bytes) {
            Ok(d) => d,
            Err(err) => {
                let backup = format!("{}.unreadable-{}", path.display(), chrono::Local::now().format("%Y%m%d-%H%M%S"));
                fs::rename(&path, &backup)
                    .with_context(|| format!("price history {} is unreadable ({err}) and could not be set aside", path.display()))?;
                let warning = format!("price history {} was unreadable ({err}); kept it as {backup} and started a new one", path.display());
                return Ok((fresh(path), Some(warning)));
            }
        };
        for (id, by_finish) in &mut data.cards {
            for (finish, dp) in by_finish {
                match dates::day_number(&dp.from) {
                    Some(start) => dp.start = start,
                    None if dp.cents.is_empty() => {}
                    None => bail!("price history {}: {id} {finish} starts on {:?}, not a date", path.display(), dp.from),
                }
            }
        }
        let points = data.points();
        Ok((PriceHistory { path, data: RwLock::new(data), saved_points: Mutex::new(points) }, None))
    }

    /// An empty history kept in memory only (for tests, or when the file can't be used).
    pub fn in_memory() -> PriceHistory {
        PriceHistory { path: PathBuf::new(), data: RwLock::default(), saved_points: Mutex::new(0) }
    }

    /// The history file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The number of recorded daily prices.
    pub fn points(&self) -> usize {
        self.data.read().unwrap().points()
    }

    /// Writes the history atomically (temp file, synced, renamed over the old one).
    /// Saves are serialized so a slower save can never replace a newer one, and a save
    /// that would hold fewer prices than are on disk is refused: history is only ever
    /// added to.
    pub fn save(&self) -> Result<()> {
        if self.path.as_os_str().is_empty() {
            return Ok(());
        }
        let mut saved = self.saved_points.lock().unwrap();
        let dir = self.path.parent().ok_or_else(|| anyhow!("no folder for {}", self.path.display()))?;
        let tmp = tempfile::Builder::new().prefix("price-history_").suffix(".tmp").tempfile_in(dir)?;
        let points;
        {
            // Streamed to the file (buffered) rather than built in memory.
            let mut w = BufWriter::with_capacity(1 << 16, tmp.as_file());
            let data = self.data.read().unwrap();
            points = data.points();
            if points < *saved {
                bail!("refusing to save price history with {points} prices over {} saved: history is never truncated", *saved);
            }
            serde_json::to_writer(&mut w, &*data)?;
            w.write_all(b"\n")?;
            w.flush()?;
        }
        tmp.as_file().sync_all()?;
        tmp.persist(&self.path)?;
        *saved = points;
        Ok(())
    }

    /// The price recorded for a printing and finish on `date` (`YYYY-MM-DD`).
    pub fn price(&self, id: &str, finish: &str, date: &str) -> Option<f64> {
        let d = dates::day_number(date)?;
        self.data.read().unwrap().cards.get(id)?.get(finish)?.at(d)
    }

    /// The number of days with a price for a printing and finish.
    pub fn days(&self, id: &str, finish: &str) -> usize {
        self.data.read().unwrap().cards.get(id).and_then(|m| m.get(finish)).map_or(0, DailyPrices::points)
    }

    /// Records one price.
    pub fn set(&self, id: &str, finish: &str, date: &str, price: f64) {
        self.data.write().unwrap().set(id, finish, date, price);
    }

    /// Adds a downloaded series for one printing (finish → date → USD). Dates already
    /// recorded keep their value (prices the app recorded itself are not replaced), and
    /// nothing is removed, so days older than the download's window stay.
    pub fn merge(&self, id: &str, by_finish: &HashMap<String, HashMap<String, f64>>) {
        let mut data = self.data.write().unwrap();
        for (finish, by_date) in by_finish {
            let days: Vec<(i64, f64)> =
                by_date.iter().filter_map(|(date, p)| dates::day_number(date).filter(|_| *p >= 0.0).map(|d| (d, *p))).collect();
            let (Some(lo), Some(hi)) = (days.iter().map(|d| d.0).min(), days.iter().map(|d| d.0).max()) else { continue };
            let dp = data.series(id, finish);
            dp.grow(lo, hi); // once, rather than per date
            for (d, p) in days {
                let slot = &mut dp.cents[(d - dp.start) as usize];
                if *slot < 0 {
                    *slot = to_cents(p);
                }
            }
        }
    }

    /// Records that price history was loaded on `date`.
    pub fn mark_history_loaded(&self, date: &str) {
        self.data.write().unwrap().history_loaded = date.to_string();
    }

    /// The date of the last price history load, or "".
    pub fn history_loaded_on(&self) -> String {
        self.data.read().unwrap().history_loaded.clone()
    }

    /// Stores each card's current market prices (every finish) for `date`.
    pub fn record_current(&self, cards: &[Card], date: &str) {
        let mut data = self.data.write().unwrap();
        for c in cards.iter().filter(|c| !c.id.is_empty()) {
            for (finish, p) in &c.market {
                data.set(&c.id, finish, date, *p);
            }
        }
    }

    /// The earliest date with any recorded price.
    pub fn start(&self) -> Option<NaiveDate> {
        let data = self.data.read().unwrap();
        data.cards
            .values()
            .flat_map(BTreeMap::values)
            .filter_map(|dp| dp.recorded().map(|(first, _)| dp.start + first as i64))
            .min()
            .map(dates::from_day)
    }

    /// The collection's market value on each day from `from` through `to` on which any
    /// price is recorded, plus the range's end points: the copies owned that day (by
    /// acquired date; undated copies always count) times that day's price for their
    /// finish. A card without a price that day uses its nearest earlier recorded price,
    /// then its nearest later one, then its current catalog price, so gaps in the data
    /// don't read as value drops.
    pub fn market_series(&self, cards: &[Card], from: NaiveDate, to: NaiveDate) -> Vec<TimePoint> {
        let data = self.data.read().unwrap();
        let (from_d, to_d) = (dates::day_of(from), dates::day_of(to));
        if to_d < from_d {
            return Vec::new();
        }
        // One item per purchase, each with its own acquired date.
        struct Item<'a> {
            qty: f64,
            /// The first day owned; undated purchases count from the start.
            acquired: i64,
            prices: Option<(&'a DailyPrices, usize, usize)>,
            now: Option<f64>,
        }
        let mut items = Vec::new();
        let mut marked = vec![false; (to_d - from_d + 1) as usize]; // days in range with a recorded price
        let mut earlier = false; // some price is from before the range
        for c in cards {
            for (finish, _, p) in c.copies.each() {
                let mut item = Item {
                    qty: p.quantity as f64,
                    acquired: dates::day_number(&p.acquired_date).unwrap_or(i64::MIN),
                    prices: None,
                    now: c.market_price(finish),
                };
                if let Some(dp) = data.cards.get(&c.id).and_then(|m| m.get(finish))
                    && let Some((first, last)) = dp.recorded()
                {
                    item.prices = Some((dp, first, last));
                    let lo = from_d.max(dp.start + first as i64);
                    let hi = to_d.min(dp.start + last as i64);
                    for d in lo..=hi {
                        if dp.cents[(d - dp.start) as usize] >= 0 {
                            marked[(d - from_d) as usize] = true;
                        }
                    }
                    earlier |= dp.start + (first as i64) < from_d;
                }
                items.push(item);
            }
        }
        // Always span the requested range: the last day (prices carried forward) and the
        // first day when there is history from before it.
        if !items.is_empty() {
            let n = marked.len();
            marked[n - 1] = true;
            marked[0] |= earlier;
        }
        let days: Vec<i64> = marked.iter().enumerate().filter(|(_, m)| **m).map(|(i, _)| from_d + i as i64).collect();

        let mut totals = vec![0.0; days.len()];
        for item in &items {
            for (k, &d) in days.iter().enumerate() {
                if d < item.acquired {
                    continue; // not owned yet
                }
                match (item.prices, item.now) {
                    (Some((dp, first, last)), _) => {
                        // That day's price, else the nearest earlier one, else the first.
                        let mut i = usize::try_from(d - dp.start).unwrap_or(0).min(last);
                        while i > first && dp.cents[i] < 0 {
                            i -= 1;
                        }
                        totals[k] += dollars(dp.cents[i.max(first)]) * item.qty;
                    }
                    (None, Some(now)) => totals[k] += now * item.qty,
                    (None, None) => {}
                }
            }
        }
        days.iter().zip(totals).map(|(d, value)| TimePoint { date: dates::from_day(*d), value }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::{Condition, Purchase};

    fn day(s: &str) -> NaiveDate {
        dates::parse(s).unwrap()
    }

    fn series(h: &PriceHistory, cards: &[Card], from: &str, to: &str) -> Vec<(String, f64)> {
        h.market_series(cards, day(from), day(to)).iter().map(|p| (dates::format(p.date), (p.value * 100.0).round() / 100.0)).collect()
    }

    fn card(id: &str, qty: i64, acquired: &str, now: Option<f64>) -> Card {
        let mut c = Card { id: id.into(), ..Card::default() };
        c.add_purchase("nonfoil", Condition::NearMint, Purchase { quantity: qty, acquired_date: acquired.into(), ..Default::default() });
        if let Some(p) = now {
            c.market.insert("nonfoil".into(), p);
        }
        c
    }

    #[test]
    fn set_grow_and_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("price-history.json");
        let (h, warn) = PriceHistory::open(&path).unwrap();
        assert!(warn.is_none());
        h.set("a", "nonfoil", "2024-01-05", 1.25);
        h.set("a", "nonfoil", "2024-01-02", 1.0);
        h.set("a", "nonfoil", "2024-01-03", -1.0); // ignored
        h.set("a", "nonfoil", "not a date", 9.0); // ignored
        assert_eq!(h.price("a", "nonfoil", "2024-01-05"), Some(1.25));
        assert_eq!(h.price("a", "nonfoil", "2024-01-03"), None);
        assert_eq!(h.days("a", "nonfoil"), 2);
        h.mark_history_loaded("2024-01-06");
        h.save().unwrap();

        let (again, _) = PriceHistory::open(&path).unwrap();
        assert_eq!(again.price("a", "nonfoil", "2024-01-02"), Some(1.0));
        assert_eq!(again.history_loaded_on(), "2024-01-06");
        assert_eq!(again.start(), Some(day("2024-01-02")));
        let raw: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(raw["cards"]["a"]["nonfoil"]["from"], "2024-01-02");
        assert_eq!(raw["cards"]["a"]["nonfoil"]["cents"], serde_json::json!([100, -1, -1, 125]));
    }

    #[test]
    fn merge_keeps_recorded_prices() {
        let h = PriceHistory::in_memory();
        h.set("a", "foil", "2024-01-02", 5.0);
        let by_finish: HashMap<String, HashMap<String, f64>> =
            [("foil".into(), [("2024-01-01".into(), 1.0), ("2024-01-02".into(), 2.0), ("2024-01-04".into(), 4.0)].into())].into();
        h.merge("a", &by_finish);
        assert_eq!(h.price("a", "foil", "2024-01-01"), Some(1.0));
        assert_eq!(h.price("a", "foil", "2024-01-02"), Some(5.0));
        assert_eq!(h.price("a", "foil", "2024-01-04"), Some(4.0));
        assert_eq!(h.points(), 3);
    }

    #[test]
    fn unreadable_file_is_set_aside() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("price-history.json");
        fs::write(&path, b"{not json").unwrap();
        let (h, warn) = PriceHistory::open(&path).unwrap();
        assert!(warn.unwrap().contains("unreadable"));
        assert_eq!(h.points(), 0);
        assert!(!path.exists());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn save_never_truncates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.json");
        fs::write(&path, br#"{"cards":{"a":{"nonfoil":{"from":"2024-01-01","cents":[100,200]}}}}"#).unwrap();
        let (h, _) = PriceHistory::open(&path).unwrap();
        h.data.write().unwrap().cards.clear();
        assert!(h.save().unwrap_err().to_string().contains("never truncated"));
    }

    #[test]
    fn market_series_fills_gaps() {
        let h = PriceHistory::in_memory();
        h.set("a", "nonfoil", "2024-01-02", 1.0);
        h.set("a", "nonfoil", "2024-01-04", 3.0);
        let cards = vec![
            card("a", 2, "", None),
            card("b", 1, "2024-01-04", Some(10.0)), // no history: today's price once owned
            card("c", 1, "", None),                 // no price at all
        ];
        assert_eq!(
            series(&h, &cards, "2024-01-01", "2024-01-05"),
            [("2024-01-02".into(), 2.0), ("2024-01-04".into(), 16.0), ("2024-01-05".into(), 16.0)]
        );
        // History from before the range starts it on the first day.
        assert_eq!(series(&h, &cards, "2024-01-03", "2024-01-03"), [("2024-01-03".into(), 2.0)]);
        assert!(series(&h, &cards, "2024-01-05", "2024-01-01").is_empty());
        assert!(series(&h, &[], "2024-01-01", "2024-01-02").is_empty());
    }
}
