package collection

import (
	"reflect"
	"testing"
)

func sampleCards() []Card {
	return []Card{
		{ID: "1", Name: "Scapeshift", Set: "M19", SetName: "Core Set 2019", Number: "201", Rarity: "rare", Quantity: 2, PurchasePrice: 40, Finish: "nonfoil"},
		{ID: "2", Name: "Lightning Bolt", Set: "2XM", SetName: "Double Masters", Number: "10", Rarity: "uncommon", Quantity: 4, PurchasePrice: 1.5, Finish: "foil"},
		{ID: "3", Name: "Sol Ring", Set: "C21", SetName: "Commander 2021", Number: "2", Rarity: "uncommon", Quantity: 1, PurchasePrice: 2, Finish: "NonFoil"},
		{ID: "4", Name: "Niv-Mizzet, Parun", Set: "GRN", SetName: "Guilds of Ravnica", Number: "192", Rarity: "rare", Quantity: 1, PurchasePrice: 3, Finish: "nonfoil"},
		{ID: "5", Name: "Lightning Bolt", Set: "2XM", SetName: "Double Masters", Number: "10a", Rarity: "uncommon", Quantity: 1, Finish: "etched"},
	}
}

func ids(cards []Card) []string {
	out := make([]string, len(cards))
	for i, c := range cards {
		out[i] = c.ID
	}
	return out
}

func TestApplyQuery(t *testing.T) {
	tests := []struct {
		name string
		q    Query
		want []string
	}{
		{"default sorts by name", Query{}, []string{"2", "5", "4", "1", "3"}},
		{"text matches name", Query{Text: "bolt"}, []string{"2", "5"}},
		{"text matches set name", Query{Text: "ravnica"}, []string{"4"}},
		{"all terms must match", Query{Text: "lightning 10a"}, []string{"5"}},
		{"set filter is case-insensitive", Query{Set: "grn"}, []string{"4"}},
		{"game-specific match", Query{Match: func(c Card) bool { return c.Set == "2XM" }}, []string{"2", "5"}},
		{"rarity", Query{Rarity: "rare"}, []string{"4", "1"}},
		{"finish ignores case", Query{Finish: "Nonfoil"}, []string{"4", "1", "3"}},
		{"any is no filter", Query{Set: FilterAny, Rarity: FilterAny}, []string{"2", "5", "4", "1", "3"}},
		{"price descending", Query{SortBy: SortPrice, Descending: true}, []string{"1", "4", "3", "2", "5"}},
		{"quantity ascending ties by name", Query{SortBy: SortQuantity}, []string{"5", "4", "3", "1", "2"}},
		{"collector number is natural", Query{SortBy: SortNumber}, []string{"3", "2", "5", "4", "1"}},
		{"rarity uses the game's order", Query{SortBy: SortRarity, RarityOrder: []string{"common", "uncommon", "rare"}}, []string{"2", "5", "3", "4", "1"}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := ids(ApplyQuery(sampleCards(), tt.q)); !reflect.DeepEqual(got, tt.want) {
				t.Errorf("got %v, want %v", got, tt.want)
			}
		})
	}
}

func TestApplyQueryDoesNotMutateInput(t *testing.T) {
	in := sampleCards()
	before := ids(in)
	ApplyQuery(in, Query{SortBy: SortPrice})
	if !reflect.DeepEqual(ids(in), before) {
		t.Fatal("input slice was reordered")
	}
}

func TestSetOptions(t *testing.T) {
	labels, byLabel := SetOptions(sampleCards())
	want := []string{FilterAny, "C21 — Commander 2021", "M19 — Core Set 2019", "2XM — Double Masters", "GRN — Guilds of Ravnica"}
	if !reflect.DeepEqual(labels, want) {
		t.Fatalf("labels = %v, want %v", labels, want)
	}
	if byLabel["GRN — Guilds of Ravnica"] != "GRN" || byLabel[FilterAny] != "" {
		t.Fatalf("bad mapping: %v", byLabel)
	}
}
