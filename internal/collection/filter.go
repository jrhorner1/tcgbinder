package collection

import (
	"sort"
	"strconv"
	"strings"
	"unicode"
)

// SortField identifies a column the collection list can be ordered by.
type SortField string

const (
	SortName     SortField = "Name"
	SortSet      SortField = "Set"
	SortNumber   SortField = "Collector #"
	SortRarity   SortField = "Rarity"
	SortQuantity SortField = "Quantity"
	SortPrice    SortField = "Purchase Price"
	SortAcquired SortField = "Acquired Date"
	SortReleased SortField = "Release Date"
)

// SortFields lists the sort options in the order they appear in the UI.
var SortFields = []SortField{SortName, SortSet, SortNumber, SortRarity, SortQuantity, SortPrice, SortAcquired, SortReleased}

// FilterAny is the "no filter" choice in filter menus.
const FilterAny = "Any"

// Query describes how to narrow and order the collection view.
// Zero values mean "no filter", sorted by name ascending.
type Query struct {
	Text   string // whitespace-separated terms; every term must match name, set code, set name or collector number
	Set    string // set code (case-insensitive); "" or FilterAny for all
	Rarity string // "" or FilterAny for all
	Finish string // "" or FilterAny for all, otherwise the finish (case-insensitive)
	// Match is an extra, game-specific condition (e.g. Magic's color filter); nil for none.
	Match func(Card) bool
	// RarityOrder ranks rarities for SortRarity, lowest first; others sort after them.
	RarityOrder []string
	SortBy      SortField // defaults to SortName
	Descending  bool
}

// ApplyQuery returns a new slice containing the cards that match q, ordered per q.
// The input slice is not modified.
func ApplyQuery(cards []Card, q Query) []Card {
	terms := strings.Fields(strings.ToLower(q.Text))
	out := make([]Card, 0, len(cards))
	for _, c := range cards {
		if matchesQuery(c, q, terms) {
			out = append(out, c)
		}
	}

	field := q.SortBy
	if field == "" {
		field = SortName
	}
	sort.SliceStable(out, func(i, j int) bool {
		cmp := compareBy(out[i], out[j], field, q.RarityOrder)
		if cmp == 0 {
			cmp = DefaultOrder(out[i], out[j])
		}
		if q.Descending {
			return cmp > 0
		}
		return cmp < 0
	})
	return out
}

func IsAny(s string) bool { return s == "" || s == FilterAny }

func matchesQuery(c Card, q Query, terms []string) bool {
	if !IsAny(q.Set) && !strings.EqualFold(c.Set, q.Set) {
		return false
	}
	if !IsAny(q.Rarity) && !strings.EqualFold(c.Rarity, q.Rarity) {
		return false
	}
	if !IsAny(q.Finish) && !strings.EqualFold(c.Finish, q.Finish) {
		return false
	}
	if q.Match != nil && !q.Match(c) {
		return false
	}
	if len(terms) > 0 {
		haystack := strings.ToLower(c.Name + "\x00" + c.Set + "\x00" + c.SetName + "\x00" + c.Number)
		for _, t := range terms {
			if !strings.Contains(haystack, t) {
				return false
			}
		}
	}
	return true
}

// compareBy returns -1, 0 or 1 comparing a and b on a single field.
func compareBy(a, b Card, field SortField, rarityOrder []string) int {
	switch field {
	case SortSet:
		return strings.Compare(strings.ToLower(a.SetName), strings.ToLower(b.SetName))
	case SortNumber:
		return compareCollectorNumber(a.Number, b.Number)
	case SortRarity:
		return compareInt(rank(rarityOrder, a.Rarity), rank(rarityOrder, b.Rarity))
	case SortQuantity:
		return compareInt(a.Quantity, b.Quantity)
	case SortPrice:
		return compareFloat(a.PurchasePrice, b.PurchasePrice)
	case SortAcquired:
		return strings.Compare(a.AcquiredDate, b.AcquiredDate)
	case SortReleased:
		return strings.Compare(a.ReleasedAt, b.ReleasedAt)
	default:
		return strings.Compare(strings.ToLower(a.Name), strings.ToLower(b.Name))
	}
}

// defaultOrder is the tiebreaker: name, then set, then collector number, then ID for determinism.
func DefaultOrder(a, b Card) int {
	if c := strings.Compare(strings.ToLower(a.Name), strings.ToLower(b.Name)); c != 0 {
		return c
	}
	if c := strings.Compare(strings.ToLower(a.Set), strings.ToLower(b.Set)); c != 0 {
		return c
	}
	if c := compareCollectorNumber(a.Number, b.Number); c != 0 {
		return c
	}
	return strings.Compare(a.ID, b.ID)
}

// compareCollectorNumber orders "2" < "10" < "10a" < "A1" by comparing the leading
// numeric part numerically and the remainder lexically.
func compareCollectorNumber(a, b string) int {
	an, ar := splitNumber(a)
	bn, br := splitNumber(b)
	switch {
	case an >= 0 && bn < 0:
		return -1
	case an < 0 && bn >= 0:
		return 1
	}
	if c := compareInt(an, bn); c != 0 {
		return c
	}
	return strings.Compare(strings.ToLower(ar), strings.ToLower(br))
}

func splitNumber(s string) (int, string) {
	i := 0
	for i < len(s) && unicode.IsDigit(rune(s[i])) {
		i++
	}
	if i == 0 {
		return -1, s
	}
	n, err := strconv.Atoi(s[:i])
	if err != nil {
		return -1, s
	}
	return n, s[i:]
}

// rank is v's position in order (case-insensitive), or len(order) if absent.
func rank(order []string, v string) int {
	for i, o := range order {
		if strings.EqualFold(o, v) {
			return i
		}
	}
	return len(order)
}

func compareInt(a, b int) int {
	switch {
	case a < b:
		return -1
	case a > b:
		return 1
	}
	return 0
}

func compareFloat(a, b float64) int {
	switch {
	case a < b:
		return -1
	case a > b:
		return 1
	}
	return 0
}

// SetOptions returns FilterAny followed by the distinct set codes present in cards,
// labelled "CODE — Set Name" and sorted by set name.
func SetOptions(cards []Card) (labels []string, codeByLabel map[string]string) {
	seen := map[string]string{}
	for _, c := range cards {
		code := strings.ToUpper(c.Set)
		if code == "" {
			continue
		}
		if _, ok := seen[code]; !ok {
			seen[code] = c.SetName
		}
	}
	codes := make([]string, 0, len(seen))
	for code := range seen {
		codes = append(codes, code)
	}
	sort.Slice(codes, func(i, j int) bool {
		return strings.ToLower(seen[codes[i]]) < strings.ToLower(seen[codes[j]])
	})
	codeByLabel = map[string]string{FilterAny: ""}
	labels = []string{FilterAny}
	for _, code := range codes {
		label := code
		if seen[code] != "" {
			label = code + " — " + seen[code]
		}
		labels = append(labels, label)
		codeByLabel[label] = code
	}
	return labels, codeByLabel
}
