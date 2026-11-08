package collection

import (
	"math"
	"path/filepath"
	"testing"
)

// TestAddCardsGroupsByPrinting: one card per printing; finishes and conditions nest
// under it, and separate purchases (price/date) stay separate.
func TestAddCardsGroupsByPrinting(t *testing.T) {
	store, _ := NewStore(filepath.Join(t.TempDir(), "c.json"))
	base := Card{ID: "shock", Name: "Shock", Language: "en"}
	created, merged, err := store.AddCards([]Card{
		held(base, "nonfoil", ConditionNearMint, Purchase{Quantity: 1, PricePaid: 0.10, AcquiredDate: "2020-01-01"}),
		held(base, "nonfoil", ConditionNearMint, Purchase{Quantity: 1, PricePaid: 0.12, AcquiredDate: "2021-01-01"}),
		held(base, "nonfoil", ConditionNearMint, Purchase{Quantity: 2, PricePaid: 0.10, AcquiredDate: "2020-01-01"}), // same purchase
		held(base, "foil", ConditionLightPlay, Purchase{Quantity: 1}),
		held(base, "NonFoil", "", Purchase{Quantity: 1}), // old spelling, no condition
	})
	if err != nil || created != 1 || merged != 4 {
		t.Fatalf("created=%d merged=%d err=%v", created, merged, err)
	}
	reloaded, _ := NewStore(store.FilePath())
	cards := reloaded.GetCollection()
	if len(cards) != 1 {
		t.Fatalf("want one card for the printing, got %d", len(cards))
	}
	c := cards[0]
	nm := c.Purchases("nonfoil", ConditionNearMint)
	if c.CopyCount() != 6 || len(nm) != 3 || nm[0].Quantity != 3 || nm[0].PricePaid != 0.10 || nm[1].PricePaid != 0.12 || nm[2].AcquiredDate != "" {
		t.Fatalf("nonfoil NM purchases = %+v", nm)
	}
	if c.ConditionCopyCount("foil", ConditionLightPlay) != 1 || c.FinishCopyCount("nonfoil") != 5 {
		t.Fatalf("copies = %+v", c.Copies)
	}
	if cost, unknown := c.PricePaid(); math.Abs(cost-0.42) > 1e-9 || unknown != 2 {
		t.Fatalf("cost = %v, unknown = %d", cost, unknown)
	}
}

func TestUpdateEntryAndPrintingMove(t *testing.T) {
	store, _ := NewStore(filepath.Join(t.TempDir(), "c.json"))
	a := held(Card{ID: "a", Name: "Bolt"}, "nonfoil", ConditionNearMint, Purchase{Quantity: 2})
	b := held(Card{ID: "b", Name: "Bolt"}, "foil", ConditionNearMint, Purchase{Quantity: 1})
	store.AddCards([]Card{a, b})

	edited := a
	edited.SetPurchases("nonfoil", ConditionNearMint, []Purchase{{Quantity: 3, PricePaid: 1}})
	edited.AddPurchase("nonfoil", ConditionLightPlay, Purchase{Quantity: 1})
	if key, merged, err := store.SaveCard("a", edited); err != nil || merged || key != "a" {
		t.Fatalf("edit: %q %v %v", key, merged, err)
	}

	// Moving card a to printing b merges its copies into b's card.
	moved := edited
	moved.ID = "b"
	if key, merged, err := store.SaveCard("a", moved); err != nil || !merged || key != "b" {
		t.Fatalf("move: %q %v %v", key, merged, err)
	}
	cards := store.GetCollection()
	if len(cards) != 1 || cards[0].CopyCount() != 5 || cards[0].FinishCopyCount("foil") != 1 {
		t.Fatalf("after move: %+v", cards)
	}

	// Removing every copy deletes the card.
	empty := cards[0]
	empty.Copies = nil
	if _, _, err := store.SaveCard("b", empty); err != nil || len(store.GetCollection()) != 0 {
		t.Fatalf("empty card should be deleted: %v", err)
	}
}
