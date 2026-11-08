package service

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/jrhorner1/mtgmanager/internal/collection"
	"github.com/jrhorner1/mtgmanager/internal/games/mtg"
)

func TestIsAppFile(t *testing.T) {
	for in, want := range map[string]bool{
		`{"cards":{}}`:                    true,
		"\ufeff  \n{\"game\":\"mtg\"}":    true,
		"Name,Set code,Quantity\nX,M19,1": false,
		"":                                false,
	} {
		if got := IsAppFile([]byte(in)); got != want {
			t.Errorf("IsAppFile(%q) = %v", in, got)
		}
	}
}

// TestImportAppFile exports one collection, imports it into another, and checks the
// entries arrive intact and merge with what's already there.
func TestImportAppFile(t *testing.T) {
	g := mtg.New()
	src, _ := collection.OpenStore(filepath.Join(t.TempDir(), "collection.json"), mtg.ID)
	shock := collection.Card{ID: "aaaaaaaa-0000-0000-0000-000000000001", Name: "Shock", Set: "M19", Number: "156",
		Finish: "nonfoil", Condition: collection.ConditionNearMint, Language: "en", Quantity: 2, PurchasePrice: 0.25,
		Market: map[string]float64{"nonfoil": 0.20}, PricesUpdated: "2026-11-07"}
	shock.SetAttr("mana_cost", "{R}")
	opt := collection.Card{ID: "aaaaaaaa-0000-0000-0000-000000000002", Name: "Opt", Set: "DOM", Number: "60",
		Finish: "foil", Condition: collection.ConditionLightPlay, Language: "en", Quantity: 1}
	src.AddCards([]collection.Card{shock, opt})
	data, err := readFile(src.FilePath())
	if err != nil {
		t.Fatal(err)
	}

	cards, report, err := ParseImport(g, data, nil)
	if err != nil || report.Format != AppFileFormat || report.Rows != 2 || len(cards) != 2 {
		t.Fatalf("import: %d cards, %+v, %v", len(cards), report, err)
	}
	dst, _ := collection.OpenStore(filepath.Join(t.TempDir(), "collection.json"), mtg.ID)
	dst.AddCards([]collection.Card{shock}) // already owns 2 Shock
	created, merged, err := dst.AddCards(cards)
	if err != nil || created != 1 || merged != 1 {
		t.Fatalf("created=%d merged=%d err=%v", created, merged, err)
	}
	got := map[string]collection.Card{}
	for _, c := range dst.GetCollection() {
		got[c.Name] = c
	}
	if got["Shock"].Quantity != 4 || got["Shock"].AttrString("mana_cost") != "{R}" || got["Shock"].Market["nonfoil"] != 0.20 {
		t.Errorf("Shock after import: %+v", got["Shock"])
	}
	if got["Opt"].Finish != "foil" || got["Opt"].Condition != collection.ConditionLightPlay {
		t.Errorf("Opt after import: %+v", got["Opt"])
	}
}

// TestImportLegacyAppFile: files written before multi-game support have no "game"
// and keep Scryfall's prices object; they import and are upgraded on the way in.
func TestImportLegacyAppFile(t *testing.T) {
	legacy := `{"cards": {"175e21a3-00f7-4c51-8a8e-fbfd7089efda|nonfoil|NM|en": {
		"id": "175e21a3-00f7-4c51-8a8e-fbfd7089efda", "name": "Scapeshift", "set": "M19", "set_name": "Core Set 2019",
		"collector_number": "201", "rarity": "mythic", "finish": "NonFoil", "condition": "NM", "quantity": 1, "lang": "en",
		"mana_cost": "{2}{G}{G}", "colors": ["G"], "prices": {"usd": "41.88", "usd_foil": "65.77"}}},
		"history": [{"date": "2026-11-07", "market": 316.24, "cost": 207.72, "copies": 957}]}`
	cards, _, err := ParseImport(mtg.New(), []byte(legacy), nil)
	if err != nil || len(cards) != 1 {
		t.Fatalf("%d cards, %v", len(cards), err)
	}
	c := cards[0]
	if c.Market["nonfoil"] != 41.88 || c.Finish != "nonfoil" || c.Attr("prices") != nil || c.AttrString("mana_cost") != "{2}{G}{G}" {
		t.Errorf("upgraded legacy card: market=%v finish=%q attrs=%v", c.Market, c.Finish, c.Attrs)
	}
}

func TestImportRefusesOtherGamesFile(t *testing.T) {
	_, _, err := ParseImport(mtg.New(), []byte(`{"game": "pokemon", "cards": {"x": {"id": "x", "quantity": 1}}}`), nil)
	if err == nil || !strings.Contains(err.Error(), "pokemon") {
		t.Fatalf("want a refusal naming the other game, got %v", err)
	}
	if _, _, err := ParseImport(mtg.New(), []byte(`{"not": "a collection"}`), nil); err == nil {
		t.Fatal("JSON without cards should be refused")
	}
}

// readFile reads a collection file and checks it is valid JSON.
func readFile(path string) ([]byte, error) {
	b, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	var v json.RawMessage
	return b, json.Unmarshal(b, &v)
}
