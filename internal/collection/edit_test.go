package collection

import (
	"path/filepath"
	"testing"
)

func TestUniqueAndTopByEach(t *testing.T) {
	bolt := held(Card{ID: "a", Name: "Bolt", Market: map[string]float64{"nonfoil": 2, "foil": 5}}, "nonfoil", "", Purchase{Quantity: 4})
	bolt = held(bolt, "foil", "", Purchase{Quantity: 1})
	cards := []Card{
		bolt,
		held(Card{ID: "b", Name: "Shift", Market: map[string]float64{"nonfoil": 40}}, "nonfoil", "", Purchase{Quantity: 1}),
		held(Card{ID: "c", Name: "Opt", Market: map[string]float64{"nonfoil": 1.5}}, "nonfoil", "", Purchase{Quantity: 30}),
	}
	if n := UniqueCards(cards); n != 3 {
		t.Fatalf("unique = %d", n)
	}
	top := TopByEach(cards, 3)
	// Bolt's one-copy value is its best owned finish (foil, $5), not its total ($13).
	if top[0].Card.Name != "Shift" || top[1].Each != 5 || top[1].Total != 13 || top[2].Each != 1.5 {
		t.Fatalf("one-copy ranking: %+v", top)
	}
}

func TestWithPrinting(t *testing.T) {
	owned := held(Card{ID: "old", Name: "Bolt", Set: "M10", Language: "en"}, "foil", ConditionLightPlay, Purchase{Quantity: 3, PricePaid: 1.25, AcquiredDate: "2020-01-02"})
	owned.SetAttr("mana_cost", "{R}")
	newer := Card{ID: "new", Name: "Bolt", Set: "2XM", CollectorNumber: "117", Finishes: []string{"nonfoil", "foil"}, Language: "en", Market: map[string]float64{"foil": 3}}
	newer.SetAttr("mana_cost", "{R}")
	c := WithPrinting(owned, newer)
	p := c.Purchases("foil", ConditionLightPlay)
	if c.ID != "new" || c.Set != "2XM" || c.CopyCount() != 3 || len(p) != 1 || p[0].PricePaid != 1.25 || p[0].AcquiredDate != "2020-01-02" {
		t.Fatalf("moved: %+v", c)
	}
	nonfoilOnly := Card{ID: "x", Finishes: []string{"nonfoil"}}
	if WithPrinting(owned, nonfoilOnly).FinishCopyCount("nonfoil") != 3 {
		t.Fatal("copies should move to a finish the printing has")
	}
}

func TestMissing(t *testing.T) {
	set := []Card{{CollectorNumber: "1"}, {CollectorNumber: "2"}, {CollectorNumber: "3"}}
	got := Missing(set, []Card{{CollectorNumber: "2"}})
	if len(got) != 2 || got[0].CollectorNumber != "1" || got[1].CollectorNumber != "3" {
		t.Fatalf("missing = %+v", got)
	}
}

func TestBulkEdit(t *testing.T) {
	store, _ := OpenStore(filepath.Join(t.TempDir(), "c.json"), "mtg")
	a := held(Card{ID: "a", Name: "a", Set: "M19", CollectorNumber: "1", Finishes: []string{"nonfoil", "foil"}, Language: "en"},
		"nonfoil", ConditionNearMint, Purchase{Quantity: 2, PricePaid: 0.10})
	a = held(a, "foil", ConditionNearMint, Purchase{Quantity: 1, PricePaid: 0.30})
	b := held(Card{ID: "b", Name: "b", Set: "M19", CollectorNumber: "2", Finishes: []string{"nonfoil"}, Language: "en"},
		"nonfoil", ConditionNearMint, Purchase{Quantity: 1})
	store.AddCards([]Card{a, b})

	lp, foil, price := ConditionLightPlay, "foil", 0.5
	res, err := store.BulkEdit([]string{"a", "b"}, BulkChange{Condition: &lp, Finish: &foil, PricePaid: &price})
	if err != nil || res.Updated != 2 || len(res.Skipped) != 1 {
		t.Fatalf("result = %+v, %v", res, err)
	}
	got := map[string]Card{}
	for _, c := range store.GetCollection() {
		got[c.ID] = c
	}
	// a's copies all move to foil LP at $0.50, joining into one purchase.
	if p := got["a"].Purchases("foil", ConditionLightPlay); got["a"].CopyCount() != 3 || len(p) != 1 || p[0].Quantity != 3 || p[0].PricePaid != 0.5 {
		t.Fatalf("a = %+v", got["a"].Copies)
	}
	// b isn't printed in foil: it keeps its finish but still gets the other changes.
	if got["b"].ConditionCopyCount("nonfoil", ConditionLightPlay) != 1 {
		t.Fatalf("b = %+v", got["b"].Copies)
	}
	if res, _ := store.BulkEdit([]string{"a", "b"}, BulkChange{Delete: true}); res.Deleted != 2 || len(store.GetCollection()) != 0 {
		t.Fatalf("bulk delete: %+v", res)
	}
}
