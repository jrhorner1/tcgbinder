package importer

import (
	"bytes"
	"encoding/csv"
	"errors"
	"fmt"
	"io"
	"strconv"
	"strings"
	"time"

	"github.com/jrhorner1/mtgmanager/internal/collection"
)

// ImportRow is one line of an external collection export, normalized but not yet
// matched to a Scryfall printing.
type Row struct {
	Line          int // 1-based line in the source file, for error reporting
	Name          string
	SetCode       string // lowercase Scryfall-style set code when known
	SetName       string
	Number        string // collector number
	ScryfallID    string
	MultiverseID  int
	MTGOID        int
	TCGPlayerID   int
	Quantity      int
	Finish        string // "nonfoil", "foil" or "etched"
	Condition     collection.Condition
	Language      string // Scryfall language code, e.g. "en", "ja"
	PurchasePrice float64
	AcquiredDate  string // YYYY-MM-DD or ""
}

// Describe is a short human label for reports, e.g. "Line 12: Lightning Bolt (2XM #117)".
func (r Row) Describe() string {
	where := strings.ToUpper(r.SetCode)
	if where == "" {
		where = r.SetName
	}
	if r.Number != "" {
		where += " · " + r.Number
	}
	name := r.Name
	if name == "" {
		name = "(no name)"
	}
	if where != "" {
		return fmt.Sprintf("Line %d: %s (%s)", r.Line, name, where)
	}
	return fmt.Sprintf("Line %d: %s", r.Line, name)
}

type importField int

const (
	fName importField = iota
	fSetCode
	fSetName
	fSetAmbiguous // a "Set"/"Edition" column that may hold either a code or a full name
	fNumber
	fScryfallID
	fMultiverseID
	fMTGOID
	fTCGPlayerID
	fQuantity
	fFinish
	fCondition
	fLanguage
	fPrice
	fDate
)

// headerAliases maps normalized header names (lowercase, '_'/'-' as spaces) to fields.
// Matching is exact, never substring, so columns like "foil_price" or "price_updated"
// can't be mistaken for the finish or purchase price.
var headerAliases = map[string]importField{
	"name": fName, "card name": fName, "card": fName, "card title": fName,

	"set code": fSetCode, "edition code": fSetCode, "setcode": fSetCode, "set id": fSetCode,

	"set name": fSetName, "expansion": fSetName, "edition name": fSetName, "expansion name": fSetName,

	"set": fSetAmbiguous, "edition": fSetAmbiguous,

	"collector number": fNumber, "card number": fNumber, "number": fNumber, "collector no": fNumber,
	"collector #": fNumber, "cn": fNumber, "card #": fNumber, "collectors number": fNumber,

	"scryfall id": fScryfallID, "scryfallid": fScryfallID, "scryfall uuid": fScryfallID, "scryfall card id": fScryfallID,

	"multiverse id": fMultiverseID, "multiverseid": fMultiverseID, "mid": fMultiverseID, "multiverse": fMultiverseID,

	"mtgo id": fMTGOID, "mtgoid": fMTGOID,

	"tcgplayer id": fTCGPlayerID, "tcgplayer product id": fTCGPlayerID, "product id": fTCGPlayerID,
	"tcg id": fTCGPlayerID, "tcgplayer productid": fTCGPlayerID,

	"quantity": fQuantity, "count": fQuantity, "qty": fQuantity, "amount": fQuantity, "total quantity": fQuantity,

	"foil": fFinish, "finish": fFinish, "printing": fFinish, "foil type": fFinish, "is foil": fFinish,

	"condition": fCondition, "cond": fCondition,

	"language": fLanguage, "lang": fLanguage,

	"purchase price": fPrice, "price acquired": fPrice, "price bought": fPrice, "price paid": fPrice,
	"acquired price": fPrice, "purchase cost": fPrice, "buy price": fPrice, "cost": fPrice,

	"date acquired": fDate, "date bought": fDate, "date added": fDate, "acquired date": fDate,
	"acquired": fDate, "purchase date": fDate, "acquired on": fDate, "added": fDate,
}

func normalizeHeader(h string) string {
	h = strings.TrimPrefix(h, "\ufeff")
	h = strings.ToLower(strings.TrimSpace(h))
	h = strings.NewReplacer("_", " ", "-", " ").Replace(h)
	return strings.Join(strings.Fields(h), " ")
}

// detectFormat names the exporting app from its header set, for the import report.
func detectFormat(norm map[string]bool) string {
	switch {
	case norm["emid"] || norm["inventory id"] && norm["price acquired"]:
		return "EchoMTG"
	case norm["manabox id"]:
		return "ManaBox"
	case norm["folder name"] && norm["price bought"]:
		return "Dragon Shield"
	case norm["tradelist count"] && norm["edition code"]:
		return "Deckbox"
	case norm["tradelist count"]:
		return "Moxfield"
	case norm["simple name"] || norm["sku"] && norm["product id"]:
		return "TCGplayer"
	case norm["scryfall id"] && norm["date added"]:
		return "Archidekt"
	case norm["list name"] && norm["edition"]:
		return "Delver Lens"
	}
	return "Generic CSV"
}

// ParseImportCSV reads a collection export from any supported app and returns its rows.
func ParseCSV(data []byte) (rows []Row, format string, err error) {
	data = bytes.TrimPrefix(data, []byte("\ufeff"))
	r := csv.NewReader(bytes.NewReader(data))
	r.LazyQuotes = true
	r.FieldsPerRecord = -1

	headers, err := r.Read()
	if err != nil {
		return nil, "", fmt.Errorf("could not read header row: %w", err)
	}
	// Dragon Shield (and Excel) exports may start with a "sep=," hint line.
	if len(headers) > 0 && strings.HasPrefix(strings.ToLower(strings.TrimSpace(headers[0])), "sep=") {
		if headers, err = r.Read(); err != nil {
			return nil, "", fmt.Errorf("could not read header row: %w", err)
		}
	}

	cols := map[importField]int{}
	seen := map[string]bool{}
	for i, h := range headers {
		n := normalizeHeader(h)
		seen[n] = true
		if f, ok := headerAliases[n]; ok {
			if _, dup := cols[f]; !dup { // first matching column wins
				cols[f] = i
			}
		}
	}
	format = detectFormat(seen)

	_, hasName := cols[fName]
	_, hasID := cols[fScryfallID]
	_, hasMID := cols[fMultiverseID]
	_, hasTCG := cols[fTCGPlayerID]
	if !hasName && !hasID && !hasMID && !hasTCG {
		return nil, format, errors.New("unrecognized file: no card name or card ID column found in the header")
	}

	get := func(rec []string, f importField) string {
		if i, ok := cols[f]; ok && i < len(rec) {
			return strings.TrimSpace(rec[i])
		}
		return ""
	}

	for {
		rec, err := r.Read()
		if err == io.EOF {
			break
		}
		if err != nil {
			continue // skip a malformed row rather than abandoning the import
		}
		line, _ := r.FieldPos(0)

		row := Row{
			Line:          line,
			Name:          get(rec, fName),
			SetCode:       strings.ToLower(get(rec, fSetCode)),
			SetName:       get(rec, fSetName),
			Number:        get(rec, fNumber),
			ScryfallID:    strings.ToLower(get(rec, fScryfallID)),
			MultiverseID:  parseIntLoose(get(rec, fMultiverseID)),
			MTGOID:        parseIntLoose(get(rec, fMTGOID)),
			TCGPlayerID:   parseIntLoose(get(rec, fTCGPlayerID)),
			Quantity:      1,
			Finish:        ParseFinish(get(rec, fFinish)),
			Condition:     collection.ParseCondition(get(rec, fCondition)),
			Language:      ParseLanguage(get(rec, fLanguage)),
			PurchasePrice: parsePrice(get(rec, fPrice)),
			AcquiredDate:  parseDate(get(rec, fDate)),
		}
		if _, ok := cols[fQuantity]; ok {
			q := parseIntLoose(get(rec, fQuantity))
			if q <= 0 {
				continue // explicit zero-count rows (e.g. wishlist entries) aren't owned cards
			}
			row.Quantity = q
		}
		if amb := get(rec, fSetAmbiguous); amb != "" {
			// With a dedicated set-code column, "Set"/"Edition" holds the set name
			// (Deckbox, TCGplayer). Otherwise guess from its shape (Moxfield uses codes).
			_, hasCodeCol := cols[fSetCode]
			switch {
			case hasCodeCol || !looksLikeSetCode(amb):
				if row.SetName == "" && !strings.EqualFold(amb, row.SetCode) {
					row.SetName = amb
				}
			case row.SetCode == "":
				row.SetCode = strings.ToLower(amb)
			}
		}
		if row.Name == "" && row.ScryfallID == "" && row.MultiverseID == 0 && row.TCGPlayerID == 0 {
			continue
		}
		rows = append(rows, row)
	}
	return rows, format, nil
}

func looksLikeSetCode(s string) bool {
	if len(s) < 2 || len(s) > 6 {
		return false
	}
	for _, r := range s {
		if !(r >= 'a' && r <= 'z' || r >= 'A' && r <= 'Z' || r >= '0' && r <= '9') {
			return false
		}
	}
	return true
}

// parseIntLoose accepts "3", "3.0" and " 3 ".
func parseIntLoose(s string) int {
	s = strings.TrimSpace(s)
	if s == "" {
		return 0
	}
	if n, err := strconv.Atoi(s); err == nil {
		return n
	}
	if f, err := strconv.ParseFloat(s, 64); err == nil {
		return int(f)
	}
	return 0
}

func parsePrice(s string) float64 {
	s = strings.TrimSpace(strings.NewReplacer("$", "", "€", "", "£", "", "USD", "", "EUR", "").Replace(s))
	if strings.Contains(s, ",") && !strings.Contains(s, ".") {
		s = strings.ReplaceAll(s, ",", ".") // European decimal comma
	} else {
		s = strings.ReplaceAll(s, ",", "")
	}
	f, err := strconv.ParseFloat(strings.TrimSpace(s), 64)
	if err != nil || f < 0 {
		return 0
	}
	return f
}

var dateLayouts = []string{
	"2006-01-02", "2006-01-02 15:04:05", time.RFC3339, "2006-01-02T15:04:05", "2006-01-02T15:04:05.000Z",
	"2006/01/02", "01/02/2006", "1/2/2006", "01/02/06", "1/2/06", "Jan 2, 2006", "2 Jan 2006",
}

// parseDate converts common export date formats to YYYY-MM-DD; unknown formats yield "".
func parseDate(s string) string {
	s = strings.TrimSpace(s)
	if s == "" {
		return ""
	}
	for _, layout := range dateLayouts {
		if t, err := time.Parse(layout, s); err == nil {
			return t.Format("2006-01-02")
		}
	}
	if len(s) >= 10 { // e.g. "2018-08-08 00:00:00 UTC"
		if t, err := time.Parse("2006-01-02", s[:10]); err == nil {
			return t.Format("2006-01-02")
		}
	}
	return ""
}
