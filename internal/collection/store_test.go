package collection

import (
	"path/filepath"
	"testing"
)

func TestUpdateEntry(t *testing.T) {
	store, _ := NewStore(filepath.Join(t.TempDir(), "c.json"))
	nonfoil := Card{ID: "x", Name: "Shock", Finish: "nonfoil", Condition: ConditionNearMint, Language: "en", Quantity: 2, PurchasePrice: 1}
	foil := nonfoil
	foil.Finish, foil.Quantity, foil.PurchasePrice = "foil", 1, 4
	_, _, _ = store.AddCards([]Card{nonfoil, foil})

	// Plain edit keeps the key.
	edited := nonfoil
	edited.Quantity = 3
	key, merged, err := store.UpdateEntry(EntryKey(nonfoil), edited)
	if err != nil || merged || key != EntryKey(nonfoil) {
		t.Fatalf("plain edit: key=%q merged=%v err=%v", key, merged, err)
	}

	// Changing the finish to one already owned merges into that entry.
	edited.Finish = "foil"
	key, merged, err = store.UpdateEntry(EntryKey(nonfoil), edited)
	if err != nil || !merged || key != EntryKey(foil) {
		t.Fatalf("finish change: key=%q merged=%v err=%v", key, merged, err)
	}
	cards := store.GetCollection()
	if len(cards) != 1 || cards[0].Quantity != 4 || cards[0].PurchasePrice != 1.75 {
		t.Fatalf("after merge: %+v", cards)
	}

	// Quantity zero deletes.
	gone := cards[0]
	gone.Quantity = 0
	if _, _, err := store.UpdateEntry(EntryKey(cards[0]), gone); err != nil || len(store.GetCollection()) != 0 {
		t.Fatalf("zero quantity should delete: %v %+v", err, store.GetCollection())
	}
}

func TestMergeEntries(t *testing.T) {
	a := Card{ID: "x", Quantity: 1, PurchasePrice: 1.00, AcquiredDate: "2020-01-01"}
	b := Card{ID: "x", Quantity: 3, PurchasePrice: 2.00, AcquiredDate: "2019-06-01", PricesUpdated: "2026-11-07", ImageURI: "new"}
	m := mergeEntries(a, b)
	if m.Quantity != 4 || m.PurchasePrice != 1.75 || m.AcquiredDate != "2019-06-01" || m.ImageURI != "new" {
		t.Fatalf("unexpected merge: %+v", m)
	}
	if m := mergeEntries(Card{Quantity: 2}, Card{Quantity: 1, PurchasePrice: 3}); m.PurchasePrice != 3 {
		t.Fatalf("unknown price should not dilute the known one: %+v", m)
	}
}

func TestStoreMergesByPrintingFinishAndCondition(t *testing.T) {
	store, err := NewStore(filepath.Join(t.TempDir(), "c.json"))
	if err != nil {
		t.Fatal(err)
	}
	base := Card{ID: "abc", Name: "Shock", Finish: "nonfoil", Condition: ConditionNearMint, Language: "en", Quantity: 1}
	foil := base
	foil.Finish = "foil"
	played := base
	played.Condition = ConditionLightPlay
	legacyCase := base
	legacyCase.Finish = "NonFoil" // older add-dialog spelling must still merge

	created, merged, err := store.AddCards([]Card{base, base, foil, played, legacyCase})
	if err != nil {
		t.Fatal(err)
	}
	if created != 3 || merged != 2 {
		t.Fatalf("created=%d merged=%d, want 3 and 2", created, merged)
	}
	// Reloading from disk keeps the same entries.
	reloaded, err := NewStore(store.FilePath())
	if err != nil {
		t.Fatal(err)
	}
	qty := map[string]int{}
	for _, c := range reloaded.GetCollection() {
		qty[c.Finish+"/"+string(c.Condition)] = c.Quantity
	}
	if qty["nonfoil/NM"] != 3 || qty["foil/NM"] != 1 || qty["nonfoil/LP"] != 1 {
		t.Fatalf("quantities after reload = %v", qty)
	}
}
