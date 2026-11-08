package mtg

import (
	"fmt"
	"log/slog"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"unicode"

	"github.com/jrhorner1/tcgbinder/internal/collection"
	"github.com/jrhorner1/tcgbinder/internal/game"
	"github.com/jrhorner1/tcgbinder/internal/scan"
)

// Card scanning. Scryfall has no image search, so a scan is identified from the text
// on the card: the name in the title bar and, on cards printed since 2014 (Magic 2015),
// the set code, language and collector number in the bottom-left corner, e.g.
//
//	0156/280 C          or, since 2023,   0012 U
//	M19 • EN  ⟩ ARTIST                    LCI • EN ⟩ ARTIST
//
// The set code and number identify the exact printing; the name confirms it, and is
// all there is to go on for older cards.

var _ game.Scanner = (*Game)(nil)

// scanClues is what a reading says about the card.
type scanClues struct {
	Name, Set, Number, Lang string
	Year                    int // copyright year, e.g. 1995 from "© 1995 Wizards of the Coast"
}

// score rates how much a reading identifies (for choosing which way up the card is).
func (c scanClues) score() int {
	s := 0
	if c.Name != "" {
		s++
	}
	if c.Set != "" {
		s += 2
	}
	if c.Number != "" {
		s += 2
	}
	return s
}

func (c scanClues) key() string {
	return strings.ToLower(c.Name) + "|" + c.Set + "|" + c.Number
}

// describe says what was read, e.g. "Shock · M19 · 156".
func (c scanClues) describe() string {
	var parts []string
	for _, p := range []string{c.Name, c.Set, c.Number} {
		if p != "" {
			parts = append(parts, p)
		}
	}
	if c.Lang != "" && c.Lang != "en" {
		parts = append(parts, strings.ToUpper(c.Lang))
	}
	if len(parts) == 0 {
		return "nothing readable"
	}
	return strings.Join(parts, " · ")
}

var (
	// "M19 • EN", "LCI · EN", "WAR*JP": a set code, a separator and a language code.
	setLangRE = regexp.MustCompile(`\b([A-Z0-9]{3,5})\s*[^A-Za-z0-9\s]{0,3}\s*(EN|ES|FR|DE|IT|PT|JP|JA|KR|KO|RU|CS|CT|ZHS|ZHT|PH)\b`)
	// "0156/280": number of the set's total (2014-2022 frames, and 2008-2014 cards).
	numberOfRE = regexp.MustCompile(`\b([0-9]{1,4})\s*/\s*[0-9]{1,4}\b`)
	// "0012 U": number then rarity letter (frames since 2023).
	numberRarityRE = regexp.MustCompile(`(?:^|\s)([A-Z]?[0-9]{1,4}[a-z]?)\s+[CURMLSTPB](?:\s|$)`)
	// "U 0260": rarity letter then number (some frames since 2023).
	rarityNumberRE = regexp.MustCompile(`(?:^|\s)[CURMLSTPB]\s+([0-9]{3,4}[a-z]?)(?:\s|$)`)
)

// langCodes maps the language codes printed on cards to Scryfall's.
var langCodes = map[string]string{
	"EN": "en", "ES": "es", "FR": "fr", "DE": "de", "IT": "it", "PT": "pt", "JP": "ja", "JA": "ja",
	"KR": "ko", "KO": "ko", "RU": "ru", "CS": "zhs", "ZHS": "zhs", "CT": "zht", "ZHT": "zht", "PH": "ph",
}

// readClues picks the name, set, language and collector number out of a reading. A
// card photographed upside down reads fine but its lines come back in flipped places,
// so the flipped layout is tried too.
func readClues(r scan.Reading) scanClues {
	c := cluesAsIs(r)
	if !r.CardFound {
		return c
	}
	flipped := cluesAsIs(flip(r))
	if flipped.score() > c.score() || (flipped.score() == c.score() && footerOnTop(r)) {
		return flipped
	}
	return c
}

// flip turns a card reading's positions upside down.
func flip(r scan.Reading) scan.Reading {
	out := scan.Reading{CardFound: r.CardFound, Lines: make([]scan.Line, len(r.Lines))}
	for i, l := range r.Lines {
		l.X, l.Y = 1-l.X-l.W, 1-l.Y-l.H
		out.Lines[i] = l
	}
	return out
}

var footerRE = regexp.MustCompile(`(?i)wizards of the coast|©|illus\.|all rights reserved`)

// footerOnTop reports that the copyright or artist line, printed at the bottom of
// every card, was read in the top half: the card is upside down.
func footerOnTop(r scan.Reading) bool {
	for _, l := range r.Lines {
		if footerRE.MatchString(l.Text) {
			return l.Y < 0.5
		}
	}
	return false
}

var yearRE = regexp.MustCompile(`(?:©|\(c\)|\bC)\s*((?:19|20)\d\d)\b|\b((?:19|20)\d\d)\s+Wizards`)

func cluesAsIs(r scan.Reading) scanClues {
	var c scanClues
	lines := r.Sorted()

	// The name: the biggest text near the top-left of the card (when the card was
	// found), otherwise the first line that looks like a name.
	var best *scan.Line
	for i := range lines {
		l := &lines[i]
		name := cleanName(l.Text)
		if letterCount(name) < 3 {
			continue
		}
		if r.CardFound {
			if l.Y+l.H/2 > 0.14 || l.X > 0.5 {
				continue
			}
			if best == nil || l.H > best.H*1.15 {
				best = l
			}
		} else if best == nil {
			best = l
		}
	}
	if best != nil {
		c.Name = cleanName(best.Text)
	}

	// The set, language and number: in the bottom-left corner of the card.
	var bottom []string
	for _, l := range lines {
		if r.CardFound && (l.Y < 0.85 || l.X > 0.6) {
			continue
		}
		bottom = append(bottom, l.Text)
	}
	for _, l := range lines {
		if m := yearRE.FindStringSubmatch(l.Text); m != nil && c.Year == 0 {
			c.Year, _ = strconv.Atoi(m[1] + m[2])
		}
	}
	for _, text := range bottom {
		if m := setLangRE.FindStringSubmatch(text); m != nil && c.Set == "" && !isAllDigits(m[1]) {
			c.Set, c.Lang = m[1], langCodes[m[2]]
		}
		if c.Number == "" {
			if m := numberOfRE.FindStringSubmatch(text); m != nil {
				c.Number = trimZeros(m[1])
			} else if m := numberRarityRE.FindStringSubmatch(text); m != nil {
				c.Number = trimZeros(m[1])
			} else if m := rarityNumberRE.FindStringSubmatch(text); m != nil {
				c.Number = trimZeros(m[1])
			}
		}
	}
	return c
}

// cleanName keeps the letters, spaces and punctuation names use, dropping what OCR
// makes of mana symbols (e.g. "Shock ®" → "Shock").
func cleanName(s string) string {
	var b strings.Builder
	for _, r := range s {
		switch {
		case unicode.IsLetter(r), r == ' ', r == '\'', r == '’', r == ',', r == '-':
			if r == '’' {
				r = '\''
			}
			b.WriteRune(r)
		default:
			b.WriteRune(' ')
		}
	}
	words := strings.Fields(b.String())
	for len(words) > 1 && len([]rune(words[len(words)-1])) == 1 { // stray symbol letters at the end
		words = words[:len(words)-1]
	}
	for len(words) > 0 && strings.Trim(words[0], ",'-") == "" {
		words = words[1:]
	}
	return strings.TrimRight(strings.Join(words, " "), ",-' ")
}

func letterCount(s string) int {
	n := 0
	for _, r := range s {
		if unicode.IsLetter(r) {
			n++
		}
	}
	return n
}

func isAllDigits(s string) bool {
	for _, r := range s {
		if r < '0' || r > '9' {
			return false
		}
	}
	return s != ""
}

// trimZeros drops the leading zeros printed on cards ("0156" → "156"), as Scryfall's
// collector numbers have none.
func trimZeros(n string) string {
	if t := strings.TrimLeft(n, "0"); t != "" {
		return t
	}
	return "0"
}

// ScanKey implements game.Scanner.
func (g *Game) ScanKey(r scan.Reading) (string, bool) {
	c := readClues(r)
	return c.key(), c.Name != "" || (c.Set != "" && c.Number != "")
}

// setVariants lists the set code as read and, when OCR may have confused letters and
// digits (O/0, I/1, ...), single-character corrections to try next.
func setVariants(code string) []string {
	out := []string{code}
	swaps := map[byte]string{'O': "0", '0': "O", 'I': "1", '1': "I", 'L': "1", 'S': "5", '5': "S", 'B': "8", '8': "B", 'Z': "2", '2': "Z"}
	for i := 0; i < len(code) && len(out) < 4; i++ {
		if to, ok := swaps[code[i]]; ok {
			out = append(out, code[:i]+to+code[i+1:])
		}
	}
	return out
}

// Identify implements game.Scanner: the printing by set and collector number when
// they were read (checked against the name), otherwise every printing of the card
// whose name is closest to the one read.
func (g *Game) Identify(r scan.Reading) (game.ScanResult, error) {
	c := readClues(r)
	res := game.ScanResult{Read: c.describe(), Name: c.Name}
	slog.Debug("scan clues", "name", c.Name, "set", c.Set, "number", c.Number, "lang", c.Lang)

	if c.Set != "" && c.Number != "" {
		for _, set := range setVariants(c.Set) {
			sc, found, err := g.Scryfall.CardByNumber(set, c.Number, c.Lang)
			if err != nil {
				return res, err
			}
			if !found {
				continue
			}
			card := FromScryfall(sc)
			// Names on cards in other languages are translated, so they can't be checked.
			if c.Name == "" || (c.Lang != "" && c.Lang != "en") || sameName(c.Name, card.Name) {
				res.Candidates, res.Exact, res.Name = []collection.Card{card}, true, card.Name
				if c.Name == "" {
					res.Note = "The name couldn't be read; this is the card with that set and collector number."
				}
				return res, nil
			}
			slog.Debug("scan: set and number disagree with name", "read", c.Name, "found", card.Name)
			break // trust the name over a misread number
		}
	}

	if c.Name == "" {
		res.Note = "The card's name couldn't be read. Try again with the whole card in view and less glare."
		return res, nil
	}
	sc, found, err := g.Scryfall.NamedFuzzy(c.Name)
	if err != nil {
		return res, err
	}
	if !found {
		res.Note = fmt.Sprintf("No card is named like %q. Try again, or search by hand.", c.Name)
		return res, nil
	}
	res.Name = sc.Name
	printings, err := g.Printings(FromScryfall(sc))
	if err != nil {
		return res, err
	}
	if len(printings) == 0 {
		printings = []collection.Card{FromScryfall(sc)}
	}
	// Printings whose set and number agree with what was read come first, then
	// (for older cards, which print neither) those released nearest the copyright year.
	score := func(p collection.Card) int {
		s := 0
		if c.Set != "" && strings.EqualFold(p.Set, c.Set) {
			s += 2
		}
		if c.Number != "" && p.CollectorNumber == c.Number {
			s++
		}
		return s
	}
	yearGap := func(p collection.Card) int {
		if c.Year == 0 || len(p.ReleasedAt) < 4 {
			return 0
		}
		y, _ := strconv.Atoi(p.ReleasedAt[:4])
		return max(y-c.Year, c.Year-y)
	}
	lang := c.Lang
	if lang == "" {
		lang = "en" // the name was read, so it's in the alphabet of an English card
	}
	otherLang := func(p collection.Card) bool { return p.Language != "" && p.Language != lang }
	sort.SliceStable(printings, func(i, j int) bool {
		if a, b := score(printings[i]), score(printings[j]); a != b {
			return a > b
		}
		if a, b := otherLang(printings[i]), otherLang(printings[j]); a != b {
			return b
		}
		return yearGap(printings[i]) < yearGap(printings[j])
	})
	res.Candidates = printings
	res.Exact = score(printings[0]) == 3
	switch {
	case res.Exact:
	case len(printings) == 1:
		res.Exact = true
	case c.Set == "" && c.Number == "":
		res.Note = "The set and collector number couldn't be read (cards before 2014 don't print them), so pick your printing."
		if c.Year != 0 {
			res.Note = fmt.Sprintf("Cards before 2014 don't print their set; printings from around %d are listed first. Check yours.", c.Year)
		}
	case score(printings[0]) >= 2:
		res.Note = "The collector number couldn't be confirmed; check the printing."
	default:
		res.Note = "The set couldn't be matched; pick your printing."
	}
	return res, nil
}

// sameName reports whether a name read by OCR is close enough to a card's name (or
// the name of one of its faces).
func sameName(read, name string) bool {
	a := normName(read)
	for _, face := range strings.Split(name, " // ") {
		b := normName(face)
		if a == b || (len(a) >= 4 && (strings.Contains(b, a) || strings.Contains(a, b))) {
			return true
		}
		if d := levenshtein(a, b); float64(d) <= 0.25*float64(max(len([]rune(a)), len([]rune(b)))) {
			return true
		}
	}
	return false
}

func normName(s string) string {
	var b strings.Builder
	for _, r := range strings.ToLower(s) {
		if unicode.IsLetter(r) {
			b.WriteRune(r)
		}
	}
	return b.String()
}

func levenshtein(a, b string) int {
	ra, rb := []rune(a), []rune(b)
	prev := make([]int, len(rb)+1)
	for j := range prev {
		prev[j] = j
	}
	for i := 1; i <= len(ra); i++ {
		cur := make([]int, len(rb)+1)
		cur[0] = i
		for j := 1; j <= len(rb); j++ {
			cost := 1
			if ra[i-1] == rb[j-1] {
				cost = 0
			}
			cur[j] = min(prev[j]+1, cur[j-1]+1, prev[j-1]+cost)
		}
		prev = cur
	}
	return prev[len(rb)]
}
