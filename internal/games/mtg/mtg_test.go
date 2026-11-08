package mtg

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/jrhorner1/tcgbinder/internal/collection"
	"github.com/jrhorner1/tcgbinder/internal/games/mtg/scryfall"
)

func TestFromScryfallKeepsMagicFieldsAsAttrs(t *testing.T) {
	data, err := os.ReadFile("scryfall/testdata/search-scapeshift.json")
	if err != nil {
		t.Fatal(err)
	}
	var resp scryfall.SearchResponse
	if err := json.Unmarshal(data, &resp); err != nil {
		t.Fatal(err)
	}
	c := FromScryfall(&resp.Data[0])
	if c.Name != "Scapeshift" || c.Set != "M19" || c.CollectorNumber != "201" || c.RulesText == "" {
		t.Fatalf("common fields: %+v", c)
	}
	if c.AttrString("mana_cost") != "{2}{G}{G}" || strings.Join(c.AttrStrings("colors"), "") != "G" || c.AttrString("oracle_id") == "" {
		t.Errorf("Magic attrs: %v", c.Attrs)
	}
	if c.Market["nonfoil"] <= 0 || c.Market["foil"] <= 0 {
		t.Errorf("market prices: %v", c.Market)
	}
}

// legacyEntry is a card exactly as versions before multi-game support saved it.
const legacyEntry = `{
  "id": "175e21a3-00f7-4c51-8a8e-fbfd7089efda",
  "oracle_id": "c235015e-a7a9-4f8d-bf4a-cf68b3847f83",
  "multiverse_ids": [447337], "mtgo_id": 68563, "arena_id": 68082, "tcgplayer_id": 168720, "cardmarket_id": 359968,
  "name": "Scapeshift", "lang": "en", "released_at": "2018-07-13",
  "image_uri": "https://cards.scryfall.io/normal/front/1/7/175e21a3.jpg",
  "mana_cost": "{2}{G}{G}", "type_line": "Sorcery", "cmc": 4, "colors": ["G"], "color_identity": ["G"],
  "finishes": ["nonfoil", "foil"], "set_id": "2f5f2509", "set": "M19", "set_name": "Core Set 2019",
  "collector_number": "201", "rarity": "mythic", "finish": "NonFoil", "condition": "NM", "quantity": 1,
  "purchase_price": 30, "acquired_date": "2018-08-08",
  "prices": {"usd": "41.88", "usd_foil": "65.77"}, "prices_updated": "2026-11-07"
}`

func TestColorFilter(t *testing.T) {
	mk := func(colors ...string) collection.Card {
		var c collection.Card
		c.SetAttr("colors", colors)
		return c
	}
	f := New().Filters()[0]
	cases := []struct {
		card collection.Card
		opt  string
		want bool
	}{
		{mk("R"), "R", true}, {mk("U", "R"), "R", true}, {mk("G"), "R", false},
		{mk(), "Colorless", true}, {mk("G"), "Colorless", false},
		{mk("U", "R"), "Multicolor", true}, {mk("R"), "Multicolor", false},
	}
	for _, c := range cases {
		if got := f.Match(c.card, c.opt); got != c.want {
			t.Errorf("%v %s = %v, want %v", c.card.AttrStrings("colors"), c.opt, got, c.want)
		}
	}
}

// TestLiveImport imports a real export against live Scryfall into a temporary
// collection; skipped unless MTG_IMPORT_FILE points at a CSV.
func TestLiveImport(t *testing.T) {
	path := os.Getenv("MTG_IMPORT_FILE")
	if path == "" {
		t.Skip("set MTG_IMPORT_FILE to run against the live Scryfall API")
	}
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	g := New()
	cards, report, err := g.Import(data, nil)
	if err != nil {
		t.Fatal(err)
	}
	store, _ := collection.OpenStore(filepath.Join(t.TempDir(), "collection.json"), ID)
	created, merged, err := store.AddCards(cards)
	if err != nil {
		t.Fatal(err)
	}
	copies := 0
	for _, c := range cards {
		copies += c.CopyCount()
	}
	t.Logf("%s: %d rows, %d cards, %d new cards, %d merged, %d notes", report.Format, report.Rows, copies, created, merged, len(report.Notes))
	// Importing again only adds to quantities.
	again, _, _ := g.Import(data, nil)
	if c2, _, _ := store.AddCards(again); c2 != 0 {
		t.Errorf("re-import created %d cards", c2)
	}
}
