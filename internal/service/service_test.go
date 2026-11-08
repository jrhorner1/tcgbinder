package service

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/jrhorner1/tcgbinder/internal/collection"
	"github.com/jrhorner1/tcgbinder/internal/games/mtg"
	"github.com/jrhorner1/tcgbinder/internal/settings"
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
// cards arrive intact and merge with what's already there.
func TestImportAppFile(t *testing.T) {
	g := mtg.New()
	src, _ := collection.OpenStore(filepath.Join(t.TempDir(), "collection.json"), mtg.ID)
	shock := held(collection.Card{ID: "aaaaaaaa-0000-0000-0000-000000000001", Name: "Shock", Set: "M19", CollectorNumber: "156", Language: "en",
		Market: map[string]float64{"nonfoil": 0.20}, PricesUpdated: "2026-11-07"}, "nonfoil", collection.ConditionNearMint, collection.Purchase{Quantity: 2, PricePaid: 0.25})
	shock.SetAttr("mana_cost", "{R}")
	opt := held(collection.Card{ID: "aaaaaaaa-0000-0000-0000-000000000002", Name: "Opt", Set: "DOM", CollectorNumber: "60", Language: "en"}, "foil", collection.ConditionLightPlay, collection.Purchase{Quantity: 1})
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
	if got["Shock"].CopyCount() != 4 || got["Shock"].AttrString("mana_cost") != "{R}" || got["Shock"].Market["nonfoil"] != 0.20 {
		t.Errorf("Shock after import: %+v", got["Shock"])
	}
	if got["Opt"].ConditionCopyCount("foil", collection.ConditionLightPlay) != 1 {
		t.Errorf("Opt after import: %+v", got["Opt"])
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

func TestPricesDueFollowsSetting(t *testing.T) {
	store, _ := collection.OpenStore(filepath.Join(t.TempDir(), "c.json"), "mtg")
	threeDaysAgo := time.Now().AddDate(0, 0, -3).Format("2006-01-02")
	store.AddCards([]collection.Card{{ID: "a", Name: "Opt", PricesUpdated: threeDaysAgo,
		Copies: collection.Copies{"nonfoil": {"NM": {{Quantity: 1}}}}}})
	svc := FromStore(mtg.New(), store)
	for refresh, want := range map[string]bool{"daily": true, "weekly": false, "manual": false} {
		s := settings.Defaults()
		s.PriceRefresh = refresh
		svc.Settings = func() settings.Settings { return s }
		if got := svc.PricesDue(); got != want {
			t.Errorf("%s with 3-day-old prices: due=%v, want %v", refresh, got, want)
		}
	}
}
