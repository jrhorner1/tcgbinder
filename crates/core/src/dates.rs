//! Calendar dates. The collection stores dates as `YYYY-MM-DD` text; charts and price
//! history work with day numbers (days since 1970-01-01), which are fast to compare.

use chrono::{Datelike, Local, NaiveDate};

/// The format of every stored date.
pub const FORMAT: &str = "%Y-%m-%d";

/// Today's date on this computer's calendar.
pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

/// Today's date as `YYYY-MM-DD`.
pub fn today_string() -> String {
    format(today())
}

/// A date as `YYYY-MM-DD`.
pub fn format(d: NaiveDate) -> String {
    d.format(FORMAT).to_string()
}

/// Parses `YYYY-MM-DD` (exactly), or `None`.
pub fn parse(s: &str) -> Option<NaiveDate> {
    day_number(s).map(from_day)
}

/// Converts `YYYY-MM-DD` to days since 1970-01-01, or `None` for anything else. It
/// reads the digits by hand: it runs in the value chart's inner loops.
pub fn day_number(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let num = |t: &[u8]| -> Option<u32> { t.iter().try_fold(0u32, |v, &c| c.is_ascii_digit().then(|| v * 10 + u32::from(c - b'0'))) };
    let (y, m, d) = (num(&b[..4])?, num(&b[5..7])?, num(&b[8..])?);
    NaiveDate::from_ymd_opt(i32::try_from(y).ok()?, m, d).map(day_of)
}

/// The day number of a date.
pub fn day_of(d: NaiveDate) -> i64 {
    i64::from(d.num_days_from_ce()) - EPOCH_FROM_CE
}

/// The date of a day number.
pub fn from_day(n: i64) -> NaiveDate {
    NaiveDate::from_num_days_from_ce_opt(i32::try_from(n + EPOCH_FROM_CE).unwrap_or(i32::MAX)).unwrap_or(NaiveDate::MAX)
}

/// 1970-01-01 counted from 0001-01-01 (as `num_days_from_ce` counts).
const EPOCH_FROM_CE: i64 = 719_163;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_numbers() {
        assert_eq!(day_number("1970-01-01"), Some(0));
        assert_eq!(day_number("1970-01-02"), Some(1));
        assert_eq!(day_number("2024-02-30"), None);
        assert_eq!(day_number("2024-1-01"), None);
        assert_eq!(day_number("abcd-ef-gh"), None);
        let d = NaiveDate::from_ymd_opt(2024, 3, 15).unwrap();
        assert_eq!(from_day(day_of(d)), d);
        assert_eq!(parse("2024-03-15"), Some(d));
    }
}
