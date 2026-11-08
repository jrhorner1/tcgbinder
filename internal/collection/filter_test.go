package collection

import (
	"reflect"
	"strings"
	"testing"
)

func sampleCards() []Card {
	return []Card{
		held(Card{ID: "1", Name: "Scapeshift", Set: "M19", SetName: "Core Set 2019", CollectorNumber: "201", Rarity: "rare"}, "nonfoil", "", Purchase{Quantity: 2, PricePaid: 40}),
		held(Card{ID: "2", Name: "Lightning Bolt", Set: "2XM", SetName: "Double Masters", CollectorNumber: "10", Rarity: "uncommon"}, "foil", "", Purchase{Quantity: 4, PricePaid: 1.5}),
		held(Card{ID: "3", Name: "Sol Ring", Set: "C21", SetName: "Commander 2021", CollectorNumber: "2", Rarity: "uncommon"}, "NonFoil", "", Purchase{Quantity: 1, PricePaid: 2}),
		held(Card{ID: "4", Name: "Niv-Mizzet, Parun", Set: "GRN", SetName: "Guilds of Ravnica", CollectorNumber: "192", Rarity: "rare"}, "nonfoil", "", Purchase{Quantity: 1, PricePaid: 3}),
		held(Card{ID: "5", Name: "Lightning Bolt", Set: "2XM", SetName: "Double Masters", CollectorNumber: "10a", Rarity: "uncommon"}, "etched", "", Purchase{Quantity: 1}),
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
		{"price paid descending (total per printing)", Query{SortBy: SortPrice, Descending: true}, []string{"1", "2", "4", "3", "5"}},
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

func TestFoldHelpers(t *testing.T) {
	for _, tc := range []struct {
		s, sub string
		want   bool
	}{
		{"Lightning Bolt", "bolt", true}, {"Lightning Bolt", "ning b", true}, {"Bolt", "bolts", false},
		{"Æther Vial", "æther", true}, {"Jötun Grunt", "jötun", true}, {"M19", "m1", true}, {"", "a", false}, {"x", "", true},
	} {
		if got := containsFold(tc.s, tc.sub); got != tc.want {
			t.Errorf("containsFold(%q, %q) = %v", tc.s, tc.sub, got)
		}
	}
	for _, pair := range [][2]string{{"apple", "Banana"}, {"Bolt", "bolt"}, {"Æther", "aether"}, {"opt", "Opt II"}, {"Zed", "ábc"}} {
		want := strings.Compare(strings.ToLower(pair[0]), strings.ToLower(pair[1]))
		if got := compareFold(pair[0], pair[1]); got != want {
			t.Errorf("compareFold(%q, %q) = %d, want %d", pair[0], pair[1], got, want)
		}
	}
}

func TestSortCards(t *testing.T) {
	cards := []Card{
		held(Card{ID: "a", Name: "Opt", Set: "XLN", CollectorNumber: "65", ReleasedAt: "2017-09-29", Market: map[string]float64{"nonfoil": 0.1}}, "nonfoil", "", Purchase{Quantity: 1}),
		held(Card{ID: "b", Name: "Shock", Set: "M19", CollectorNumber: "156", ReleasedAt: "2018-07-13", Market: map[string]float64{"nonfoil": 0.2}}, "nonfoil", "", Purchase{Quantity: 1}),
		held(Card{ID: "c", Name: "Bolt", Set: "M19", CollectorNumber: "12", ReleasedAt: "2018-07-13", Market: map[string]float64{"nonfoil": 2}}, "nonfoil", "", Purchase{Quantity: 1}),
	}
	ids := func(cs []Card) string {
		var s []string
		for _, c := range cs {
			s = append(s, c.ID)
		}
		return strings.Join(s, ",")
	}
	for order, want := range map[string]string{
		OrderName: "c,a,b", OrderNewest: "c,b,a", OrderOldest: "a,c,b", OrderValue: "c,b,a", "": "c,a,b",
	} {
		if got := ids(SortCards(cards, order)); got != want {
			t.Errorf("%q order = %s, want %s", order, got, want)
		}
	}
	if ids(cards) != "a,b,c" {
		t.Error("SortCards changed its input")
	}
}
