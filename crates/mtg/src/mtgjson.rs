//! Daily card price history from MTGJSON (<https://mtgjson.com>), which publishes the
//! last 90 days of prices for every card as open data.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::io::{BufReader, Read};
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use reqwest::blocking::{Client as HttpClient, Response};
use serde::de::{DeserializeSeed, IgnoredAny, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};

use crate::scryfall::USER_AGENT;

/// Finish ("nonfoil", "foil", "etched") → date (`YYYY-MM-DD`) → USD.
pub type DailyPrices = HashMap<String, HashMap<String, f64>>;

/// Downloads MTGJSON files.
pub struct Client {
    pub base_url: String,
    http: OnceLock<HttpClient>,
}

impl Default for Client {
    fn default() -> Self {
        Client::new("https://mtgjson.com/api/v5")
    }
}

/// An HTTP error answer.
#[derive(Debug)]
struct Status(u16, String);

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "mtgjson {}: {}", self.1, self.0)
    }
}

impl std::error::Error for Status {}

impl Client {
    pub fn new(base_url: &str) -> Client {
        Client { base_url: base_url.trim_end_matches('/').to_string(), http: OnceLock::new() }
    }

    fn get(&self, path: &str) -> Result<Response> {
        let http = self.http.get_or_init(|| {
            // AllPrices is ~50 MB compressed.
            HttpClient::builder().user_agent(USER_AGENT).timeout(Duration::from_secs(15 * 60)).build().expect("HTTP client")
        });
        let resp = http.get(format!("{}/{path}", self.base_url)).send().with_context(|| format!("mtgjson {path}"))?;
        if !resp.status().is_success() {
            return Err(Status(resp.status().as_u16(), path.to_string()).into());
        }
        Ok(resp)
    }

    /// Maps Scryfall IDs to MTGJSON UUIDs by reading the set files for `set_codes`.
    /// Token sets such as "TM19" have no file of their own; their cards are in the
    /// parent set's file ("M19"), which is tried when the token set's file is missing.
    pub fn uuids(&self, set_codes: &[String]) -> Result<HashMap<String, String>> {
        #[derive(Deserialize)]
        struct SetFile {
            data: SetData,
        }
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct SetData {
            cards: Vec<Entry>,
            tokens: Vec<Entry>,
        }
        #[derive(Deserialize)]
        struct Entry {
            uuid: String,
            #[serde(default)]
            identifiers: Identifiers,
        }
        #[derive(Deserialize, Default)]
        #[serde(default, rename_all = "camelCase")]
        struct Identifiers {
            scryfall_id: String,
        }

        let mut out = HashMap::new();
        let mut fetched = HashSet::new();
        let mut first_err = None;
        for code in set_codes {
            let code = code.to_uppercase();
            let mut tries = vec![code.clone()];
            if code.len() > 3 && code.starts_with('T') {
                tries.push(code[1..].to_string());
            }
            for cand in tries {
                if fetched.contains(&cand) {
                    break;
                }
                let file = self.get(&format!("{cand}.json.gz")).and_then(|resp| {
                    let set: SetFile = serde_json::from_reader(BufReader::new(flate2::read::GzDecoder::new(resp)))
                        .with_context(|| format!("mtgjson {cand}"))?;
                    Ok(set)
                });
                match file {
                    Err(e) if e.downcast_ref::<Status>().is_some_and(|s| s.0 == 404) => continue, // try the parent set
                    Err(e) => {
                        fetched.insert(cand);
                        first_err.get_or_insert(e);
                        break;
                    }
                    Ok(set) => {
                        fetched.insert(cand);
                        for e in set.data.cards.into_iter().chain(set.data.tokens) {
                            if !e.identifiers.scryfall_id.is_empty() {
                                out.insert(e.identifiers.scryfall_id, e.uuid);
                            }
                        }
                        break;
                    }
                }
            }
        }
        match first_err {
            Some(e) if out.is_empty() => Err(e),
            _ => Ok(out),
        }
    }

    /// Downloads AllPrices (about 90 days of daily prices for every card) and returns
    /// the retail prices for the wanted MTGJSON UUIDs. `progress` gets the compressed
    /// bytes downloaded and the total. The file is streamed, so memory stays small.
    pub fn price_history(&self, want: &HashSet<String>, progress: &mut dyn FnMut(u64, u64)) -> Result<HashMap<String, DailyPrices>> {
        let resp = self.get("AllPrices.json.xz")?;
        let total = resp.content_length().unwrap_or(0);
        let counting = Counting { inner: resp, read: 0, last: 0, total, progress };
        let mut reader = BufReader::with_capacity(1 << 16, xz2::read::XzDecoder::new(counting));
        let out = parse_all_prices(&mut reader, want)?;
        // Read to the end: the JSON stops before the stream's end, where xz checks the
        // download's integrity (and progress reaches the total).
        std::io::copy(&mut reader, &mut std::io::sink()).context("AllPrices download")?;
        Ok(out)
    }
}

/// Reports download progress about every megabyte.
struct Counting<'a, R> {
    inner: R,
    read: u64,
    last: u64,
    total: u64,
    progress: &'a mut dyn FnMut(u64, u64),
}

impl<R: Read> Read for Counting<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.read += n as u64;
        if self.read - self.last > 1 << 20 || n == 0 {
            self.last = self.read;
            (self.progress)(self.read, self.total);
        }
        Ok(n)
    }
}

/// The providers, most preferred first. TCGplayer matches Scryfall's USD prices; Card
/// Kingdom and Mana Pool fill gaps for cards TCGplayer doesn't list.
const PROVIDERS: &[&str] = &["tcgplayer", "cardkingdom", "manapool"];

/// The part of an AllPrices card entry used.
#[derive(Deserialize, Default)]
#[serde(default)]
struct PriceEntry {
    /// Provider → its prices.
    paper: HashMap<String, Provider>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Provider {
    /// normal | foil | etched → date → price.
    retail: HashMap<String, HashMap<String, f64>>,
}

/// Takes each finish from the most preferred provider that has it.
fn pick_prices(mut e: PriceEntry) -> DailyPrices {
    let mut out = DailyPrices::new();
    for (theirs, ours) in [("normal", "nonfoil"), ("foil", "foil"), ("etched", "etched")] {
        for p in PROVIDERS {
            if let Some(series) = e.paper.get_mut(*p).and_then(|prov| prov.retail.remove(theirs)).filter(|s| !s.is_empty()) {
                out.insert(ours.to_string(), series);
                break;
            }
        }
    }
    out
}

/// Reads an AllPrices JSON document, keeping only the wanted UUIDs. Unwanted entries
/// are skipped without being decoded.
pub fn parse_all_prices(r: impl Read, want: &HashSet<String>) -> Result<HashMap<String, DailyPrices>> {
    struct Root<'a>(&'a HashSet<String>);
    struct Data<'a>(&'a HashSet<String>);

    impl<'de> DeserializeSeed<'de> for Root<'_> {
        type Value = HashMap<String, DailyPrices>;
        fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
            d.deserialize_map(self)
        }
    }
    impl<'de> Visitor<'de> for Root<'_> {
        type Value = HashMap<String, DailyPrices>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("an AllPrices document")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut out = HashMap::new();
            while let Some(key) = map.next_key::<String>()? {
                if key == "data" {
                    out = map.next_value_seed(Data(self.0))?;
                } else {
                    map.next_value::<IgnoredAny>()?;
                }
            }
            Ok(out)
        }
    }
    impl<'de> DeserializeSeed<'de> for Data<'_> {
        type Value = HashMap<String, DailyPrices>;
        fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
            d.deserialize_map(self)
        }
    }
    impl<'de> Visitor<'de> for Data<'_> {
        type Value = HashMap<String, DailyPrices>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("prices by card UUID")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut out = HashMap::new();
            while let Some(uuid) = map.next_key::<String>()? {
                if !self.0.contains(&uuid) {
                    map.next_value::<IgnoredAny>()?;
                    continue;
                }
                let prices = pick_prices(map.next_value()?);
                if !prices.is_empty() {
                    out.insert(uuid, prices);
                }
            }
            Ok(out)
        }
    }

    let mut de = serde_json::Deserializer::from_reader(r);
    let out = Root(want).deserialize(&mut de).map_err(|e| anyhow!("unexpected AllPrices format: {e}"))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;
    use crate::testing::{FakeServer, Reply};

    const ALL_PRICES: &str = r#"{
      "meta": {"date": "2026-11-07", "version": "5.3.0"},
      "data": {
        "uuid-shock": {
          "mtgo": {"cardhoarder": {"retail": {"normal": {"2026-11-07": 0.03}}}},
          "paper": {
            "tcgplayer":   {"buylist": {}, "retail": {"normal": {"2026-11-06": 0.20, "2026-11-07": 0.25}, "foil": {"2026-11-07": 1.10}}, "currency": "USD"},
            "cardkingdom": {"retail": {"normal": {"2026-11-07": 0.35}, "etched": {"2026-11-07": 3.00}}, "currency": "USD"}
          }
        },
        "uuid-other": {"paper": {"tcgplayer": {"retail": {"normal": {"2026-11-07": 9.99}}}}},
        "uuid-ckonly": {"paper": {"cardkingdom": {"retail": {"normal": {"2026-11-05": 1.50}}}}}
      }
    }"#;

    fn want(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_all_prices() {
        let got = parse_all_prices(ALL_PRICES.as_bytes(), &want(&["uuid-shock", "uuid-ckonly"])).unwrap();
        assert!(!got.contains_key("uuid-other"), "an unwanted card was kept");
        let shock = &got["uuid-shock"];
        assert_eq!(shock["nonfoil"]["2026-11-06"], 0.20, "TCGplayer prices should win");
        assert_eq!(shock["nonfoil"]["2026-11-07"], 0.25);
        assert_eq!(shock["foil"]["2026-11-07"], 1.10);
        assert_eq!(shock["etched"]["2026-11-07"], 3.00, "a finish TCGplayer lacks comes from Card Kingdom");
        assert_eq!(got["uuid-ckonly"]["nonfoil"]["2026-11-05"], 1.50);
        assert!(parse_all_prices(&b"[1]"[..], &want(&[])).is_err());
    }

    #[test]
    fn uuids_and_price_history_over_http() {
        let mut xz = xz2::write::XzEncoder::new(Vec::new(), 6);
        xz.write_all(ALL_PRICES.as_bytes()).unwrap();
        let xz = xz.finish().unwrap();
        let xz_len = xz.len() as u64;
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(
            br#"{"data":{"code":"M19",
              "cards":[{"uuid":"uuid-shock","identifiers":{"scryfallId":"sf-shock"}}],
              "tokens":[{"uuid":"uuid-angel","identifiers":{"scryfallId":"sf-angel"}}]}}"#,
        )
        .unwrap();
        let gz = gz.finish().unwrap();
        let srv = FakeServer::start(move |req| match req.path.as_str() {
            "/M19.json.gz" => Reply::bytes(gz.clone()),
            "/AllPrices.json.xz" => Reply::bytes(xz.clone()),
            _ => Reply::not_found(),
        });
        let c = Client::new(&srv.url);
        // TM19 has no file; its tokens come from M19's file.
        let ids = c.uuids(&["m19".into(), "TM19".into(), "ZZZ".into()]).unwrap();
        assert_eq!(ids.len(), 2);
        assert_eq!((ids["sf-shock"].as_str(), ids["sf-angel"].as_str()), ("uuid-shock", "uuid-angel"));

        let mut last = 0;
        let prices = c.price_history(&want(&["uuid-shock"]), &mut |read, _| last = read).unwrap();
        assert_eq!(prices["uuid-shock"]["nonfoil"]["2026-11-07"], 0.25);
        assert_eq!(last, xz_len, "progress should reach the full download");
    }
}
