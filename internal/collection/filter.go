package collection

import (
	"slices"
	"sort"
	"strconv"
	"strings"
	"unicode"
	"unicode/utf8"
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
	var matched []int
	for i := range cards {
		if matchesQuery(&cards[i], q, terms) {
			matched = append(matched, i)
		}
	}

	field := q.SortBy
	if field == "" {
		field = SortName
	}
	return sortedCards(cards, matched, func(a, b *Card) int {
		cmp := compareBy(a, b, field, q.RarityOrder)
		if cmp == 0 {
			cmp = DefaultOrder(*a, *b)
		}
		if q.Descending {
			return -cmp
		}
		return cmp
	})
}

// sortedCards returns cards[i] for each i in idx, stably ordered by cmp. It sorts
// the indexes rather than the cards, which are large structs to move.
func sortedCards(cards []Card, idx []int, cmp func(a, b *Card) int) []Card {
	slices.SortStableFunc(idx, func(i, j int) int { return cmp(&cards[i], &cards[j]) })
	out := make([]Card, len(idx))
	for k, i := range idx {
		out[k] = cards[i]
	}
	return out
}

func IsAny(s string) bool { return s == "" || s == FilterAny }

func matchesQuery(c *Card, q Query, terms []string) bool {
	if !IsAny(q.Set) && !strings.EqualFold(c.Set, q.Set) {
		return false
	}
	if !IsAny(q.Rarity) && !strings.EqualFold(c.Rarity, q.Rarity) {
		return false
	}
	if !IsAny(q.Finish) && !c.HasFinish(q.Finish) {
		return false
	}
	if q.Match != nil && !q.Match(*c) {
		return false
	}
	for _, t := range terms { // each term in the name, set code, set name or number
		if !containsFold(c.Name, t) && !containsFold(c.Set, t) && !containsFold(c.SetName, t) && !containsFold(c.CollectorNumber, t) {
			return false
		}
	}
	return true
}

// containsFold reports whether s contains sub (already lowercase), ignoring case,
// without allocating for ASCII text.
func containsFold(s, sub string) bool {
	if len(sub) > len(s) {
		return false
	}
	for i := 0; i < len(s); i++ {
		if s[i] >= utf8.RuneSelf {
			return strings.Contains(strings.ToLower(s), sub)
		}
	}
	for i := 0; i+len(sub) <= len(s); i++ {
		j := 0
		for j < len(sub) && lowerASCII(s[i+j]) == sub[j] {
			j++
		}
		if j == len(sub) {
			return true
		}
	}
	return false
}

func lowerASCII(c byte) byte {
	if 'A' <= c && c <= 'Z' {
		return c + 'a' - 'A'
	}
	return c
}

// compareFold compares strings as strings.Compare(strings.ToLower(a), strings.ToLower(b))
// does, without allocating for ASCII text.
func compareFold(a, b string) int {
	for i := 0; i < len(a) && i < len(b); i++ {
		ca, cb := a[i], b[i]
		if ca >= utf8.RuneSelf || cb >= utf8.RuneSelf {
			return strings.Compare(strings.ToLower(a[i:]), strings.ToLower(b[i:]))
		}
		if ca, cb = lowerASCII(ca), lowerASCII(cb); ca != cb {
			return compareInt(int(ca), int(cb))
		}
	}
	return compareInt(len(a), len(b))
}

// compareBy returns -1, 0 or 1 comparing a and b on a single field.
func compareBy(a, b *Card, field SortField, rarityOrder []string) int {
	switch field {
	case SortSet:
		return compareFold(a.SetName, b.SetName)
	case SortNumber:
		return compareCollectorNumber(a.CollectorNumber, b.CollectorNumber)
	case SortRarity:
		return compareInt(rank(rarityOrder, a.Rarity), rank(rarityOrder, b.Rarity))
	case SortQuantity:
		return compareInt(a.CopyCount(), b.CopyCount())
	case SortPrice:
		ac, _ := a.PricePaid()
		bc, _ := b.PricePaid()
		return compareFloat(ac, bc)
	case SortAcquired:
		return strings.Compare(a.EarliestAcquired(), b.EarliestAcquired())
	case SortReleased:
		return strings.Compare(a.ReleasedAt, b.ReleasedAt)
	default:
		return compareFold(a.Name, b.Name)
	}
}

// defaultOrder is the tiebreaker: name, then set, then collector number, then ID for determinism.
func DefaultOrder(a, b Card) int {
	if c := compareFold(a.Name, b.Name); c != 0 {
		return c
	}
	if c := compareFold(a.Set, b.Set); c != 0 {
		return c
	}
	if c := compareCollectorNumber(a.CollectorNumber, b.CollectorNumber); c != 0 {
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
