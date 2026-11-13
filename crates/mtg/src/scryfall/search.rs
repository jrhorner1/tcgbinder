use anyhow::{Result, bail};
use reqwest::Method;
use serde::{Deserialize, Serialize};

use super::{Card, Client, status_of};

/// The answer to /cards/search.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchResponse {
    pub total_cards: usize,
    pub has_more: bool,
    pub next_page: String,
    pub data: Vec<Card>,
}

/// One page of /cards/search results.
#[derive(Clone, Debug, Default)]
pub struct SearchPage {
    pub cards: Vec<Card>,
    pub total: usize,
    /// The full URL of the next page, "" on the last.
    pub next_page: String,
}

/// Combines free text with an optional set code into Scryfall syntax. Every printing is
/// returned (searches ask for unique=prints), so a set code narrows the results to that
/// set's printings.
pub fn build_search_query(text: &str, set_code: &str) -> Result<String> {
    let text = text.trim();
    let set = set_code.trim().to_lowercase();
    if !set.is_empty() && !is_set_code_input(&set) {
        bail!("{set:?} is not a set code (letters and digits, e.g. M19)");
    }
    Ok(match (text.is_empty(), set.is_empty()) {
        (true, true) => bail!("enter a card name or a set code"),
        (false, true) => text.to_string(),
        (true, false) => format!("set:{set}"),
        (false, false) => format!("{text} set:{set}"),
    })
}

fn is_set_code_input(s: &str) -> bool {
    (2..=8).contains(&s.chars().count()) && s.chars().all(char::is_alphanumeric)
}

fn encode(params: &[(&str, &str)]) -> String {
    url::form_urlencoded::Serializer::new(String::new()).extend_pairs(params).finish()
}

impl Client {
    /// Runs a search returning every printing, ordered by name and then newest first.
    /// Pass the previous page's `next_page` to continue; a search with no matches gives
    /// an empty page, not an error.
    pub fn search_cards(&self, query: &str, next_page: &str) -> Result<SearchPage> {
        self.search(&encode(&[("q", query), ("unique", "prints"), ("order", "name")]), next_page)
    }

    /// Every printing matching `query` in collector-number order, following up to
    /// `max_pages` result pages (of 175 cards). Extras and variations are included, so
    /// a set query lists every card Scryfall counts in the set.
    pub fn search_all(&self, query: &str, max_pages: usize) -> Result<Vec<Card>> {
        let params =
            encode(&[("q", query), ("unique", "prints"), ("order", "set"), ("include_extras", "true"), ("include_variations", "true")]);
        let mut out = Vec::new();
        let mut next = String::new();
        for _ in 0..max_pages {
            let page = self.search(&params, &next)?;
            out.extend(page.cards);
            if page.next_page.is_empty() {
                break;
            }
            next = page.next_page;
        }
        Ok(out)
    }

    fn search(&self, params: &str, next_page: &str) -> Result<SearchPage> {
        let path = if next_page.is_empty() {
            format!("/cards/search?{params}")
        } else {
            match next_page.strip_prefix(&self.base_url) {
                Some(rest) if rest.starts_with('/') => rest.to_string(),
                _ => bail!("unexpected next page URL {next_page:?}"),
            }
        };
        match self.request::<SearchResponse>(Method::GET, &path, None, true) {
            Ok(resp) => Ok(SearchPage {
                cards: resp.data,
                total: resp.total_cards,
                next_page: if resp.has_more { resp.next_page } else { String::new() },
            }),
            Err(e) if status_of(&e) == Some(404) => Ok(SearchPage::default()), // Scryfall's "nothing matched"
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_queries() {
        assert_eq!(build_search_query(" Bolt ", "").unwrap(), "Bolt");
        assert_eq!(build_search_query("", "M19").unwrap(), "set:m19");
        assert_eq!(build_search_query("Shock", "m19").unwrap(), "Shock set:m19");
        assert!(build_search_query("", "").is_err());
        assert!(build_search_query("x", "m-19").is_err());
        assert!(build_search_query("x", "a").is_err());
    }
}
