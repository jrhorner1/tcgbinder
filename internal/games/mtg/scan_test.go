package mtg

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/jrhorner1/tcgbinder/internal/games/mtg/scryfall"
	"github.com/jrhorner1/tcgbinder/internal/scan"
)

// card builds a reading of a card found and straightened in the photo: the title,
// the type line, and the two bottom-left info lines.
func cardReading(title string, info ...string) scan.Reading {
	r := scan.Reading{CardFound: true, Lines: []scan.Line{
		{Text: title, X: 0.07, Y: 0.045, W: 0.5, H: 0.04},
		{Text: "{2}", X: 0.85, Y: 0.045, W: 0.08, H: 0.04}, // mana cost
		{Text: "Instant", X: 0.07, Y: 0.56, W: 0.3, H: 0.035},
		{Text: "Shock deals 2 damage to any target.", X: 0.08, Y: 0.64, W: 0.8, H: 0.03},
		{Text: "™ & © 2018 Wizards of the Coast", X: 0.6, Y: 0.95, W: 0.35, H: 0.015},
	}}
	for i, t := range info {
		r.Lines = append(r.Lines, scan.Line{Text: t, X: 0.05, Y: 0.93 + float64(i)*0.025, W: 0.35, H: 0.015})
	}
	return r
}

func TestReadClues(t *testing.T) {
	for _, tc := range []struct {
		name string
		r    scan.Reading
		want scanClues
	}{
		{"2014-2022 frame", cardReading("Shock", "0156/280 C", "M19 • EN ⟩ JUNG PARK"), scanClues{"Shock", "M19", "156", "en", 2018}},
		{"2023 frame", cardReading("Scampering Surveyor", "0260 C", "LCI • EN SARAH FINNIGAN"), scanClues{"Scampering Surveyor", "LCI", "260", "en", 2018}},
		{"rarity first", cardReading("Scampering Surveyor", "U 0260", "LCI • EN IO XAVIER RIBEIRO"), scanClues{"Scampering Surveyor", "LCI", "260", "en", 2018}},
		{"mana symbols and Japanese", cardReading("Opt ® ⓦ", "060/269 C", "DOM * JP"), scanClues{"Opt", "DOM", "60", "ja", 2018}},
		{"curly apostrophe", cardReading("Urza’s Ruinous Blast"), scanClues{"Urza's Ruinous Blast", "", "", "", 2018}},
		{"older card: name and year", cardReading("Llanowar Elves", "Illus. Anson Maddocks"), scanClues{"Llanowar Elves", "", "", "", 2018}},
		{"upside down", flip(cardReading("Shock", "0156/280 C", "M19 • EN")), scanClues{"Shock", "M19", "156", "en", 2018}},
		{"no outline found", scan.Reading{Lines: []scan.Line{
			{Text: "Shock", Y: 0.2}, {Text: "Instant", Y: 0.5}, {Text: "156/280 C", Y: 0.8}, {Text: "M19 • EN", Y: 0.82},
		}}, scanClues{"Shock", "M19", "156", "en", 0}},
	} {
		if got := readClues(tc.r); got != tc.want {
			t.Errorf("%s: %+v, want %+v", tc.name, got, tc.want)
		}
	}
}

func TestScanKeyNeedsSomethingToLookUp(t *testing.T) {
	g := New()
	if _, ok := g.ScanKey(scan.Reading{CardFound: true, Lines: []scan.Line{{Text: "Instant", Y: 0.56, H: 0.03}}}); ok {
		t.Fatal("a reading without a name or number shouldn't be complete")
	}
	if k, ok := g.ScanKey(cardReading("Shock", "0156/280 C", "M19 • EN")); !ok || k != "shock|M19|156" {
		t.Fatalf("key = %q %v", k, ok)
	}
}

func TestSameName(t *testing.T) {
	for read, name := range map[string]string{
		"Shock": "Shock", "Lightning Bo1t": "Lightning Bolt", "Delver of Secrets": "Delver of Secrets // Insectile Aberration",
		"Scapeshitt": "Scapeshift",
	} {
		if !sameName(read, name) {
			t.Errorf("%q should match %q", read, name)
		}
	}
	if sameName("Shock", "Opt") || sameName("Lightning Bolt", "Lightning Helix") {
		t.Error("different names matched")
	}
}

// fakeScryfall serves /cards/{set}/{number}, /cards/named?fuzzy= and the printings search.
func fakeScryfall(t *testing.T) (*Game, *[]string) {
	t.Helper()
	shock := func(set, num string) scryfall.Card {
		return scryfall.Card{ID: "id-" + set + "-" + num, OracleID: "o-shock", Name: "Shock", Set: set, SetName: strings.ToUpper(set),
			Number: num, Rarity: "common", Finishes: []string{"nonfoil", "foil"}, ReleasedAt: map[string]string{"m19": "2018-07-13", "2xm": "2020-08-07", "akh": "2017-04-28"}[set]}
	}
	var calls []string
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		calls = append(calls, r.URL.RequestURI())
		write := func(v any) { json.NewEncoder(w).Encode(v) }
		switch {
		case r.URL.Path == "/cards/m19/156":
			write(shock("m19", "156"))
		case r.URL.Path == "/cards/named" && strings.EqualFold(r.URL.Query().Get("fuzzy"), "shok"):
			write(shock("akh", "144"))
		case r.URL.Path == "/cards/search" && r.URL.Query().Get("q") == "oracleid:o-shock":
			write(map[string]any{"data": []scryfall.Card{shock("2xm", "141"), shock("m19", "156"), shock("akh", "144")}, "total_cards": 3})
		default:
			http.NotFound(w, r)
		}
	}))
	t.Cleanup(srv.Close)
	c := scryfall.NewClient()
	c.BaseURL, c.FastGap, c.SlowGap = srv.URL, 0, 0
	return &Game{Scryfall: c}, &calls
}

func TestIdentify(t *testing.T) {
	g, calls := fakeScryfall(t)

	// Set and number read, name agrees: exactly that printing, in one request.
	res, err := g.Identify(cardReading("Shock", "0156/280 C", "M19 • EN"))
	if err != nil || !res.Exact || len(res.Candidates) != 1 || res.Candidates[0].ID != "id-m19-156" || len(*calls) != 1 {
		t.Fatalf("by number: %+v %v, calls %v", res, err, *calls)
	}

	// The set code misread as "MI9": the corrected code is tried.
	*calls = nil
	res, _ = g.Identify(cardReading("Shock", "0156/280 C", "MI9 • EN"))
	if !res.Exact || res.Candidates[0].ID != "id-m19-156" {
		t.Fatalf("misread set: %+v (calls %v)", res, *calls)
	}

	// Only a misspelled name: fuzzy match, then every printing to choose from.
	res, err = g.Identify(cardReading("Shok"))
	if err != nil || res.Exact || len(res.Candidates) != 3 || res.Name != "Shock" || res.Note == "" {
		t.Fatalf("by name: %+v %v", res, err)
	}

	// Name plus a set with the number unreadable: that set's printing comes first.
	res, _ = g.Identify(cardReading("Shok", "", "AKH • EN"))
	if res.Candidates[0].Set != "AKH" {
		t.Fatalf("set preference: first = %s", res.Candidates[0].Set)
	}

	// Nothing that matches.
	res, err = g.Identify(cardReading("Xyzzy Plugh"))
	if err != nil || len(res.Candidates) != 0 || !strings.Contains(res.Note, "Xyzzy Plugh") {
		t.Fatalf("no match: %+v %v", res, err)
	}
}
