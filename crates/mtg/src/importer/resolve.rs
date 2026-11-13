use std::collections::{BTreeMap, HashMap, HashSet};

use serde_json::{Map, Value, json};

use super::Row;
use crate::scryfall::{self, COLLECTION_BATCH_SIZE, Client};

/// The kinds of lookup, most precise first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Method {
    Id,
    SetNumber,
    Multiverse,
    Mtgo,
    Tcgplayer,
    NameSet,
    Name,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Id => "id",
            Method::SetNumber => "set+number",
            Method::Multiverse => "multiverse",
            Method::Mtgo => "mtgo",
            Method::Tcgplayer => "tcgplayer",
            Method::NameSet => "name+set",
            Method::Name => "name",
        }
    }
}

/// One way of asking Scryfall for a printing, in /cards/collection form.
#[derive(Clone, Debug)]
struct Identifier {
    method: Method,
    body: Map<String, Value>,
    /// The identifier pins one printing; otherwise the match may be a different
    /// printing than the one actually owned.
    exact: bool,
}

impl Identifier {
    fn new(method: Method, body: Value, exact: bool) -> Identifier {
        let Value::Object(body) = body else { unreachable!("identifiers are objects") };
        Identifier { method, body, exact }
    }

    fn key(&self) -> String {
        format!("{}:{}", self.method.as_str(), Value::Object(self.body.clone()))
    }

    fn str(&self, k: &str) -> String {
        value_str(self.body.get(k).unwrap_or(&Value::Null)).to_lowercase()
    }
}

/// A JSON value as text: strings without quotes, numbers as whole numbers.
fn value_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.as_i64().map_or_else(|| n.as_f64().map_or_else(String::new, |f| (f as i64).to_string()), |i| i.to_string()),
        other => other.to_string(),
    }
}

/// The lookups to try for a row, most precise first.
fn candidates(r: &Row) -> Vec<Identifier> {
    let mut out = Vec::new();
    if scryfall::is_uuid(&r.scryfall_id) {
        out.push(Identifier::new(Method::Id, json!({"id": r.scryfall_id}), true));
    }
    if !r.set_code.is_empty() && !r.number.is_empty() && r.number.len() <= 16 {
        out.push(Identifier::new(Method::SetNumber, json!({"set": r.set_code, "collector_number": r.number}), true));
    }
    if r.multiverse_id > 0 && r.multiverse_id <= 999_999 {
        // Scryfall rejects longer IDs (EchoMTG exports internal ones).
        out.push(Identifier::new(Method::Multiverse, json!({"multiverse_id": r.multiverse_id}), true));
    }
    if r.mtgo_id > 0 {
        out.push(Identifier::new(Method::Mtgo, json!({"mtgo_id": r.mtgo_id}), true));
    }
    if r.tcgplayer_id > 0 {
        out.push(Identifier::new(Method::Tcgplayer, json!({"tcgplayer_id": r.tcgplayer_id}), true));
    }
    let name = front_face(&clean_card_name(&r.name)).to_string();
    if !r.name.is_empty() && !r.set_code.is_empty() {
        // Exact only if the set has a single printing of that name; reported as approximate.
        out.push(Identifier::new(Method::NameSet, json!({"name": name, "set": r.set_code}), false));
    }
    if !r.name.is_empty() {
        out.push(Identifier::new(Method::Name, json!({"name": name}), false));
    }
    out
}

/// Strips decorations some apps add to names, e.g. EchoMTG's "Forest (266)" art-variant
/// suffix, "Knight Token" and "Emblem - Tezzeret, Artifice Master".
pub fn clean_card_name(name: &str) -> String {
    let mut name = name.trim().to_string();
    while name.ends_with(')') {
        match name.rfind(" (") {
            Some(i) if i > 0 => name = name[..i].trim().to_string(),
            _ => break,
        }
    }
    if let Some(rest) = name.strip_prefix("Emblem - ") {
        name = format!("{rest} Emblem"); // EchoMTG "Emblem - X" vs Scryfall "X Emblem"
    }
    if let Some(rest) = name.strip_prefix("Checklist Card - ") {
        name = format!("{rest} Checklist"); // EchoMTG "Checklist Card - Ixalan" vs Scryfall "Ixalan Checklist"
    }
    if name.to_lowercase().ends_with(" token") {
        name = name[..name.len() - " token".len()].trim().to_string();
    }
    name
}

/// The front face's name of a multi-faced card ("A // B" → "A").
pub fn front_face(name: &str) -> &str {
    match name.find(" // ") {
        Some(i) if i > 0 => &name[..i],
        _ => name,
    }
}

/// An import row paired with the Scryfall printing it was matched to.
#[derive(Clone, Debug)]
pub struct Resolved {
    pub row: Row,
    /// `None` when unresolved.
    pub card: Option<scryfall::Card>,
    /// The lookup that produced the match.
    pub method: Option<Method>,
    /// Matched only by name: the printing may differ from the one owned.
    pub approximate: bool,
    /// Why it couldn't be resolved.
    pub error: String,
}

/// Matches every row to a Scryfall printing. Each row's identifiers are tried in order,
/// accepting a match only when the returned card's name agrees with the row's (when
/// the export has one), so a wrong collector number in the file falls through to the
/// next identifier instead of importing the wrong card. `progress` gets the rows
/// settled so far.
pub fn resolve(c: &Client, mut rows: Vec<Row>, progress: &mut dyn FnMut(usize, usize)) -> Vec<Resolved> {
    fill_set_codes(c, &mut rows);
    let total = rows.len();
    let cand: Vec<Vec<Identifier>> = rows.iter().map(candidates).collect();
    let mut results: Vec<Resolved> =
        rows.into_iter().map(|row| Resolved { row, card: None, method: None, approximate: false, error: String::new() }).collect();
    let mut next = vec![0usize; total];
    let mut pending: Vec<usize> = Vec::new();
    for (i, ids) in cand.iter().enumerate() {
        if ids.is_empty() {
            results[i].error = "no usable name, set/number or ID".into();
        } else {
            pending.push(i);
        }
    }
    let mut settled = total - pending.len();
    progress(settled, total);

    while !pending.is_empty() {
        // Gather this round's distinct lookups.
        let mut lookups: BTreeMap<String, Identifier> = BTreeMap::new();
        let mut order = Vec::new();
        for &i in &pending {
            let id = &cand[i][next[i]];
            let key = id.key();
            if let std::collections::btree_map::Entry::Vacant(e) = lookups.entry(key) {
                order.push(e.key().clone());
                e.insert(id.clone());
            }
        }
        let found = lookup(c, &order, &lookups, &mut || progress(settled, total));

        let mut still = Vec::new();
        for i in pending {
            let id = &cand[i][next[i]];
            let card = found.get(&id.key());
            match card {
                Some(card) if names_match(&results[i].row.name, card) => {
                    results[i].card = Some(card.clone());
                    results[i].method = Some(id.method);
                    results[i].approximate = !id.exact;
                    settled += 1;
                    continue;
                }
                Some(card) => tracing::debug!(
                    row = results[i].row.describe(),
                    lookup = id.method.as_str(),
                    got = card.name,
                    "import lookup mismatch, trying the next identifier"
                ),
                None => {}
            }
            next[i] += 1;
            if next[i] >= cand[i].len() {
                results[i].error = "no matching card found on Scryfall".into();
                settled += 1;
            } else {
                still.push(i);
            }
        }
        pending = still;
        progress(settled, total);
    }
    apply_languages(c, &mut results);
    results
}

/// Fetches each identifier, batching the ones /cards/collection supports.
fn lookup(c: &Client, order: &[String], ids: &BTreeMap<String, Identifier>, tick: &mut dyn FnMut()) -> HashMap<String, scryfall::Card> {
    let mut found = HashMap::new();
    let mut batch: Vec<&str> = Vec::new();
    let mut flush = |batch: &mut Vec<&str>, found: &mut HashMap<String, scryfall::Card>| {
        if batch.is_empty() {
            return;
        }
        let bodies: Vec<Map<String, Value>> = batch.iter().map(|k| ids[*k].body.clone()).collect();
        match c.fetch_collection(&bodies) {
            Err(e) => tracing::warn!(error = %e, "scryfall collection request failed"),
            Ok(resp) => {
                // Scryfall returns found cards in request order, leaving out the
                // not_found ones. Each pairing is checked against the identifier, and the
                // batch searched if the order assumption fails.
                let missing: HashSet<String> = resp.not_found.iter().map(canonical_body).collect();
                let mut di = 0;
                for k in batch.iter() {
                    let id = &ids[*k];
                    if missing.contains(&canonical_body(&id.body)) {
                        continue;
                    }
                    if di < resp.data.len() && matches_identifier(id, &resp.data[di]) {
                        found.insert(k.to_string(), resp.data[di].clone());
                        di += 1;
                    } else if let Some(card) = resp.data.iter().find(|card| matches_identifier(id, card)) {
                        found.insert(k.to_string(), card.clone());
                    }
                }
            }
        }
        batch.clear();
        tick();
    };
    for k in order {
        let id = &ids[k];
        if id.method == Method::Tcgplayer {
            // Not supported by /cards/collection; one request each.
            if let Some(n) = id.body.get("tcgplayer_id").and_then(Value::as_i64)
                && let Ok(Some(card)) = c.card_by_tcgplayer(n)
            {
                found.insert(k.clone(), card);
            }
            continue;
        }
        batch.push(k);
        if batch.len() == COLLECTION_BATCH_SIZE {
            flush(&mut batch, &mut found);
        }
    }
    flush(&mut batch, &mut found);
    found
}

/// Whether a returned card is the one the identifier asked for.
fn matches_identifier(id: &Identifier, card: &scryfall::Card) -> bool {
    match id.method {
        Method::Id => card.id.eq_ignore_ascii_case(&id.str("id")),
        Method::SetNumber => {
            card.set.eq_ignore_ascii_case(&id.str("set")) && card.collector_number.eq_ignore_ascii_case(&id.str("collector_number"))
        }
        Method::Multiverse => card.multiverse_ids.iter().any(|m| m.to_string() == id.str("multiverse_id")),
        Method::Mtgo => card.mtgo_id.to_string() == id.str("mtgo_id"),
        Method::NameSet => card.set.eq_ignore_ascii_case(&id.str("set")) && names_match(&id.str("name"), card),
        Method::Name => names_match(&id.str("name"), card),
        Method::Tcgplayer => false,
    }
}

/// An identifier with normalized values, so request and not_found entries compare
/// equal (numbers may come back as floats, set codes in another case).
fn canonical_body(m: &Map<String, Value>) -> String {
    let norm: BTreeMap<&str, String> = m.iter().map(|(k, v)| (k.as_str(), value_str(v).to_lowercase())).collect();
    serde_json::to_string(&norm).unwrap_or_default()
}

/// Resolves set names to codes for rows that only have a set name.
fn fill_set_codes(c: &Client, rows: &mut [Row]) {
    if !rows.iter().any(|r| r.set_code.is_empty() && !r.set_name.is_empty()) {
        return;
    }
    let sets = match c.fetch_sets() {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "could not load the set list");
            return;
        }
    };
    let by_name: HashMap<String, String> = sets.values().map(|s| (normalize_name(&s.name), s.code.to_lowercase())).collect();
    for r in rows.iter_mut().filter(|r| r.set_code.is_empty() && !r.set_name.is_empty()) {
        r.set_code = by_name.get(&normalize_name(&r.set_name)).cloned().unwrap_or_default();
    }
}

/// Swaps in the non-English printing for rows owned in another language:
/// /cards/collection always answers with the English printing for set+number lookups.
fn apply_languages(c: &Client, results: &mut [Resolved]) {
    let mut cache: HashMap<(String, String, String), Option<scryfall::Card>> = HashMap::new();
    for r in results.iter_mut() {
        let lang = r.row.language.clone();
        let Some(card) = &r.card else { continue };
        if lang.is_empty() || lang == card.lang || r.method == Some(Method::Id) {
            continue;
        }
        let key = (card.set.clone(), card.collector_number.clone(), lang.clone());
        let fetched = cache
            .entry(key)
            .or_insert_with(|| match c.card_by_number(&card.set, &card.collector_number, &lang) {
                Ok(Some(found)) => Some(found),
                _ => {
                    tracing::info!(row = r.row.describe(), lang, kept = card.lang, "no printing in the requested language");
                    None
                }
            })
            .clone();
        if let Some(found) = fetched {
            r.card = Some(found);
        }
    }
}

/// Whether an export's card name refers to the Scryfall card, ignoring case,
/// punctuation, accents and which face of a double-faced card was used.
pub fn names_match(row_name: &str, card: &scryfall::Card) -> bool {
    if normalize_name(row_name).is_empty() {
        return true; // nothing to check against (ID-only exports)
    }
    let mut names = vec![normalize_name(&card.name), normalize_name(front_face(&card.name))];
    names.extend(card.card_faces.iter().map(|f| normalize_name(&f.name)));
    [normalize_name(row_name), normalize_name(&clean_card_name(row_name))].iter().any(|want| names.contains(want))
}

/// Folds common accents to their plain letter.
fn fold_accent(c: char) -> &'static str {
    match c {
        'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' => "a",
        'é' | 'è' | 'ê' | 'ë' => "e",
        'í' | 'ì' | 'î' | 'ï' => "i",
        'ó' | 'ò' | 'ô' | 'ö' | 'õ' => "o",
        'ú' | 'ù' | 'û' | 'ü' => "u",
        'ñ' => "n",
        'ç' => "c",
        'æ' => "ae",
        'œ' => "oe",
        _ => "",
    }
}

/// Keeps only lowercase letters and digits, with common accents folded.
pub fn normalize_name(s: &str) -> String {
    let mut out = String::new();
    for c in s.to_lowercase().chars() {
        match fold_accent(c) {
            "" if c.is_alphanumeric() => out.push(c),
            "" => {}
            folded => out.push_str(folded),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{FakeServer, Reply};

    #[test]
    fn card_names() {
        for (raw, want) in [
            ("Forest (266)", "Forest"),
            ("Knight Token (001)", "Knight"),
            ("Emblem - Tezzeret, Artifice Master", "Tezzeret, Artifice Master Emblem"),
            ("Checklist Card - Ixalan", "Ixalan Checklist"),
            ("Lightning Bolt", "Lightning Bolt"),
        ] {
            assert_eq!(clean_card_name(raw), want);
        }
        assert_eq!(normalize_name("Æther Vial, Jötun!"), "aethervialjotun");
        let card = scryfall::Card { name: "Delver of Secrets // Insectile Aberration".into(), ..Default::default() };
        assert!(names_match("delver of secrets", &card));
        assert!(names_match("", &card));
        assert!(!names_match("Shock", &card));
    }

    fn sf(id: &str, name: &str, set: &str, number: &str, mids: &[i64]) -> scryfall::Card {
        scryfall::Card {
            id: id.into(),
            name: name.into(),
            set: set.into(),
            collector_number: number.into(),
            multiverse_ids: mids.to_vec(),
            lang: "en".into(),
            ..Default::default()
        }
    }

    /// Serves /cards/collection from a small catalog, mimicking Scryfall's
    /// 400-on-a-malformed-batch behavior and its not_found reports.
    pub(crate) fn collection_server(catalog: Vec<scryfall::Card>) -> FakeServer {
        FakeServer::start(move |req| {
            if req.path != "/cards/collection" {
                return Reply::not_found();
            }
            let body: Value = serde_json::from_slice(&req.body).unwrap_or_default();
            let mut resp = scryfall::CollectionResponse::default();
            for id in body["identifiers"].as_array().cloned().unwrap_or_default() {
                if id["multiverse_id"].as_i64().is_some_and(|m| m > 999_999) {
                    return Reply::status(400, r#"{"details":"multiverse_id too long"}"#);
                }
                let hit = catalog.iter().find(|c| {
                    if id.get("set").is_some() && id.get("collector_number").is_some() {
                        c.set == id["set"] && c.collector_number == id["collector_number"]
                    } else if let Some(m) = id.get("multiverse_id") {
                        m.as_i64().is_some_and(|m| c.multiverse_ids.contains(&m))
                    } else if let Some(n) = id["name"].as_str() {
                        front_face(&c.name).eq_ignore_ascii_case(n) && id.get("set").is_none_or(|s| c.set == *s)
                    } else {
                        false
                    }
                });
                match hit {
                    Some(c) => resp.data.push(c.clone()),
                    None => resp.not_found.push(id.as_object().cloned().unwrap_or_default()),
                }
            }
            Reply::json(&resp)
        })
    }

    #[test]
    fn fallbacks_and_validation() {
        let srv = collection_server(vec![
            sf("1", "Shock", "m19", "156", &[100]),
            sf("2", "Forest", "dom", "266", &[]),
            sf("3", "Opt", "dom", "60", &[300]),
            sf("4", "Nicol Bolas, the Ravager // Nicol Bolas, the Arisen", "m19", "218", &[]),
        ]);
        let client = Client::unlimited(&srv.url);
        let row = |name: &str, set: &str, number: &str, mid: i64| Row {
            name: name.into(),
            set_code: set.into(),
            number: number.into(),
            multiverse_id: mid,
            language: "en".into(),
            quantity: 1,
            ..Row::default()
        };
        let rows = vec![
            row("Shock", "m19", "156", 0),                 // exact set+number
            row("Forest (266)", "dom", "266", 0),          // a decorated name still validates
            row("Opt", "dom", "156", 300),                 // wrong number → multiverse fallback
            row("Opt", "dom", "999", 123_456_789),         // a bad multiverse ID must not sink the batch
            row("Nicol Bolas, the Ravager", "m19", "", 0), // front face name+set
            row("Not A Card", "zzz", "1", 0),
        ];
        let mut last = (0, 0);
        let res = resolve(&client, rows, &mut |done, total| last = (done, total));
        assert_eq!(last, (6, 6));
        let got: Vec<(&str, &str)> =
            res.iter().map(|r| (r.card.as_ref().map_or("", |c| c.id.as_str()), r.method.map_or("", Method::as_str))).collect();
        assert_eq!(got, [("1", "set+number"), ("2", "set+number"), ("3", "multiverse"), ("3", "name+set"), ("4", "name+set"), ("", "")]);
        assert!(res[4].approximate && !res[0].approximate);
        assert_eq!(res[5].error, "no matching card found on Scryfall");
    }
}
