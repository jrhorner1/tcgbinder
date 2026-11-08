package importer

import (
	"encoding/json"
	"fmt"
	"log/slog"
	"net/url"
	"strings"
	"unicode"

	"github.com/jrhorner1/tcgbinder/internal/games/mtg/scryfall"
)

// identifier is one way of asking Scryfall for a printing, in /cards/collection form.
type identifier struct {
	kind string // "id", "set+number", "multiverse", "mtgo", "tcgplayer", "name+set", "name"
	body map[string]any
	// exact is true when the identifier pins one printing; false means the match
	// may be a different printing than the one actually owned.
	exact bool
}

func (id identifier) key() string {
	b, _ := json.Marshal(id.body)
	return id.kind + ":" + string(b)
}

// candidates lists the lookups to try for a row, most precise first.
func candidates(r Row) []identifier {
	var out []identifier
	if scryfall.IsUUID(r.ScryfallID) {
		out = append(out, identifier{"id", map[string]any{"id": r.ScryfallID}, true})
	}
	if r.SetCode != "" && r.Number != "" && len(r.Number) <= 16 {
		out = append(out, identifier{"set+number", map[string]any{"set": r.SetCode, "collector_number": r.Number}, true})
	}
	if r.MultiverseID > 0 && r.MultiverseID <= 999999 { // Scryfall rejects longer IDs (EchoMTG exports internal ones)
		out = append(out, identifier{"multiverse", map[string]any{"multiverse_id": r.MultiverseID}, true})
	}
	if r.MTGOID > 0 {
		out = append(out, identifier{"mtgo", map[string]any{"mtgo_id": r.MTGOID}, true})
	}
	if r.TCGPlayerID > 0 {
		out = append(out, identifier{"tcgplayer", map[string]any{"tcgplayer_id": r.TCGPlayerID}, true})
	}
	if r.Name != "" && r.SetCode != "" {
		// Exact only if the set has a single printing of that name; reported as approximate.
		out = append(out, identifier{"name+set", map[string]any{"name": frontFace(cleanCardName(r.Name)), "set": r.SetCode}, false})
	}
	if r.Name != "" {
		out = append(out, identifier{"name", map[string]any{"name": frontFace(cleanCardName(r.Name))}, false})
	}
	return out
}

// cleanCardName strips decorations some apps add to names, e.g. EchoMTG's
// "Forest (266)" art-variant suffix, "Knight Token" and "Emblem - Tezzeret, Artifice Master".
func cleanCardName(name string) string {
	name = strings.TrimSpace(name)
	for strings.HasSuffix(name, ")") {
		i := strings.LastIndex(name, " (")
		if i <= 0 {
			break
		}
		name = strings.TrimSpace(name[:i])
	}
	if rest, ok := strings.CutPrefix(name, "Emblem - "); ok {
		name = rest + " Emblem" // EchoMTG "Emblem - X" vs Scryfall "X Emblem"
	}
	if rest, ok := strings.CutPrefix(name, "Checklist Card - "); ok {
		name = rest + " Checklist" // EchoMTG "Checklist Card - Ixalan" vs Scryfall "Ixalan Checklist"
	}
	if strings.HasSuffix(strings.ToLower(name), " token") {
		name = strings.TrimSpace(name[:len(name)-len(" token")])
	}
	return name
}

func frontFace(name string) string {
	if i := strings.Index(name, " // "); i > 0 {
		return name[:i]
	}
	return name
}

// ResolvedRow is an import row paired with the Scryfall printing it was matched to.
type Resolved struct {
	Row         Row
	Card        *scryfall.Card // nil when unresolved
	Method      string         // identifier kind that produced the match
	Approximate bool           // matched only by name; printing may differ from the one owned
	Err         string         // why it could not be resolved
}

// ResolveRows matches every row to a Scryfall printing. It tries each row's identifiers
// in order, accepting a match only when the returned card's name agrees with the row's
// name (when the export has one), so a wrong collector number in the source file falls
// through to the next identifier instead of importing the wrong card.
// progress, if set, is called with the number of rows settled so far.
func Resolve(c *scryfall.Client, rows []Row, progress func(done, total int)) []Resolved {
	results := make([]Resolved, len(rows))
	cand := make([][]identifier, len(rows))
	next := make([]int, len(rows))
	pending := make([]int, 0, len(rows))

	fillSetCodes(c, rows)
	for i, r := range rows {
		results[i].Row = r
		cand[i] = candidates(rows[i])
		if len(cand[i]) == 0 {
			results[i].Err = "no usable name, set/number or ID"
			continue
		}
		pending = append(pending, i)
	}
	settled := len(rows) - len(pending)
	report := func() {
		if progress != nil {
			progress(settled, len(rows))
		}
	}
	report()

	for len(pending) > 0 {
		// Gather this round's distinct lookups.
		lookups := map[string]identifier{}
		var order []string
		for _, i := range pending {
			id := cand[i][next[i]]
			if _, ok := lookups[id.key()]; !ok {
				lookups[id.key()] = id
				order = append(order, id.key())
			}
		}
		found := lookup(c, order, lookups, func() { report() })

		var still []int
		for _, i := range pending {
			id := cand[i][next[i]]
			card := found[id.key()]
			if card != nil && namesMatch(rows[i].Name, card) {
				results[i].Card = card
				results[i].Method = id.kind
				results[i].Approximate = !id.exact
				settled++
				continue
			}
			if card != nil {
				slog.Debug("import lookup mismatch, trying next identifier", "row", rows[i].Describe(), "lookup", id.kind, "got", card.Name)
			}
			next[i]++
			if next[i] >= len(cand[i]) {
				results[i].Err = "no matching card found on Scryfall"
				settled++
				continue
			}
			still = append(still, i)
		}
		pending = still
		report()
	}

	applyLanguages(c, results)
	return results
}

// lookup fetches each identifier, batching the ones /cards/collection supports.
func lookup(c *scryfall.Client, order []string, ids map[string]identifier, tick func()) map[string]*scryfall.Card {
	found := map[string]*scryfall.Card{}
	var batch []string
	flush := func() {
		if len(batch) == 0 {
			return
		}
		bodies := make([]map[string]any, len(batch))
		for i, k := range batch {
			bodies[i] = ids[k].body
		}
		resp, err := c.FetchCollection(bodies)
		if err != nil {
			slog.Warn("scryfall collection request failed", "err", err)
			batch = batch[:0]
			return
		}
		// Scryfall returns found cards in request order, omitting the not_found ones.
		missing := map[string]bool{}
		for _, nf := range resp.NotFound {
			missing[canonicalBody(nf)] = true
		}
		// Verify each pairing against the identifier, and search the batch if the order assumption fails.
		di := 0
		for _, k := range batch {
			id := ids[k]
			if missing[canonicalBody(id.body)] {
				continue
			}
			if di < len(resp.Data) && matchesIdentifier(id, &resp.Data[di]) {
				card := resp.Data[di]
				found[k] = &card
				di++
				continue
			}
			for j := range resp.Data {
				if matchesIdentifier(id, &resp.Data[j]) {
					card := resp.Data[j]
					found[k] = &card
					break
				}
			}
		}
		batch = batch[:0]
		tick()
	}
	for _, k := range order {
		id := ids[k]
		if id.kind == "tcgplayer" {
			// Not supported by /cards/collection; one request each.
			var card scryfall.Card
			if _, err := c.Do("GET", fmt.Sprintf("/cards/tcgplayer/%v", id.body["tcgplayer_id"]), nil, &card, false); err == nil {
				found[k] = &card
			}
			continue
		}
		batch = append(batch, k)
		if len(batch) == scryfall.CollectionBatchSize {
			flush()
		}
	}
	flush()
	return found
}

// matchesIdentifier checks that a returned card is the one the identifier asked for.
func matchesIdentifier(id identifier, card *scryfall.Card) bool {
	str := func(k string) string { return strings.ToLower(fmt.Sprint(id.body[k])) }
	switch id.kind {
	case "id":
		return strings.EqualFold(card.ID, str("id"))
	case "set+number":
		return strings.EqualFold(card.Set, str("set")) && strings.EqualFold(card.Number, str("collector_number"))
	case "multiverse":
		for _, m := range card.MultiverseIDs {
			if fmt.Sprint(m) == str("multiverse_id") {
				return true
			}
		}
		return false
	case "mtgo":
		return fmt.Sprint(card.MTGOID) == str("mtgo_id")
	case "name+set":
		return strings.EqualFold(card.Set, str("set")) && namesMatch(str("name"), card)
	case "name":
		return namesMatch(str("name"), card)
	}
	return false
}

// canonicalBody renders an identifier map with normalized values so request and
// not_found entries compare equal (JSON numbers decode as float64, set codes may change case).
func canonicalBody(m map[string]any) string {
	norm := map[string]string{}
	for k, v := range m {
		switch x := v.(type) {
		case float64:
			norm[k] = fmt.Sprintf("%d", int64(x))
		case int:
			norm[k] = fmt.Sprintf("%d", x)
		default:
			norm[k] = strings.ToLower(fmt.Sprint(x))
		}
	}
	b, _ := json.Marshal(norm) // map keys marshal sorted
	return string(b)
}

// fillSetCodes resolves set names to codes for rows that only have a set name.
func fillSetCodes(c *scryfall.Client, rows []Row) {
	need := false
	for _, r := range rows {
		if r.SetCode == "" && r.SetName != "" {
			need = true
			break
		}
	}
	if !need {
		return
	}
	sets, err := c.FetchSets()
	if err != nil {
		slog.Warn("could not load set list", "err", err)
		return
	}
	byName := make(map[string]string, len(sets))
	for _, s := range sets {
		byName[normalizeName(s.Name)] = strings.ToLower(s.Code)
	}
	for i := range rows {
		if rows[i].SetCode == "" && rows[i].SetName != "" {
			rows[i].SetCode = byName[normalizeName(rows[i].SetName)]
		}
	}
}

// applyLanguages swaps in the non-English printing for rows owned in another language.
// /cards/collection always answers with the English printing for set+number lookups.
func applyLanguages(c *scryfall.Client, results []Resolved) {
	cache := map[string]*scryfall.Card{}
	for i := range results {
		r := &results[i]
		lang := r.Row.Language
		if r.Card == nil || lang == "" || lang == r.Card.Language || r.Method == "id" {
			continue
		}
		path := fmt.Sprintf("/cards/%s/%s/%s", url.PathEscape(r.Card.Set), url.PathEscape(r.Card.Number), url.PathEscape(lang))
		card, ok := cache[path]
		if !ok {
			var fetched scryfall.Card
			if _, err := c.Do("GET", path, nil, &fetched, false); err == nil {
				card = &fetched
			} else {
				slog.Info("no printing in the requested language", "row", r.Row.Describe(), "lang", lang, "kept", r.Card.Language)
			}
			cache[path] = card
		}
		if card != nil {
			r.Card = card
		}
	}
}

// namesMatch reports whether an export's card name refers to the Scryfall card,
// ignoring case, punctuation, accents and which face of a double-faced card was used.
func namesMatch(rowName string, card *scryfall.Card) bool {
	if normalizeName(rowName) == "" {
		return true // nothing to check against (ID-only exports)
	}
	names := []string{normalizeName(card.Name), normalizeName(frontFace(card.Name))}
	for _, f := range card.CardFaces {
		names = append(names, normalizeName(f.Name))
	}
	for _, want := range []string{normalizeName(rowName), normalizeName(cleanCardName(rowName))} {
		for _, n := range names {
			if want == n {
				return true
			}
		}
	}
	return false
}

var accentFold = strings.NewReplacer(
	"á", "a", "à", "a", "â", "a", "ä", "a", "ã", "a", "å", "a",
	"é", "e", "è", "e", "ê", "e", "ë", "e",
	"í", "i", "ì", "i", "î", "i", "ï", "i",
	"ó", "o", "ò", "o", "ô", "o", "ö", "o", "õ", "o",
	"ú", "u", "ù", "u", "û", "u", "ü", "u",
	"ñ", "n", "ç", "c", "æ", "ae", "œ", "oe",
)

// normalizeName keeps only lowercase letters and digits, with common accents folded.
func normalizeName(s string) string {
	s = accentFold.Replace(strings.ToLower(s))
	var b strings.Builder
	for _, r := range s {
		if unicode.IsLetter(r) || unicode.IsDigit(r) {
			b.WriteRune(r)
		}
	}
	return b.String()
}
