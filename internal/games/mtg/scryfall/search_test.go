package scryfall_test

import (
	"testing"

	"github.com/jrhorner1/mtgmanager/internal/games/mtg/scryfall"
	"github.com/jrhorner1/mtgmanager/internal/games/mtg/scryfall/scryfalltest"
)

func TestBuildSearchQuery(t *testing.T) {
	tests := []struct{ text, set, want string }{
		{"Lightning Bolt", "", "Lightning Bolt"},
		{"bolt", "2XM", "bolt set:2xm"},
		{"", "m19", "set:m19"},
		{"  shock ", " M19 ", "shock set:m19"},
	}
	for _, tt := range tests {
		got, err := scryfall.BuildSearchQuery(tt.text, tt.set)
		if err != nil || got != tt.want {
			t.Errorf("scryfall.BuildSearchQuery(%q, %q) = %q, %v; want %q", tt.text, tt.set, got, err, tt.want)
		}
	}
	for _, bad := range [][2]string{{"", ""}, {"bolt", "m 19"}, {"bolt", "set:x"}} {
		if _, err := scryfall.BuildSearchQuery(bad[0], bad[1]); err == nil {
			t.Errorf("scryfall.BuildSearchQuery(%q, %q) should fail", bad[0], bad[1])
		}
	}
}

func TestSearchCardsPaging(t *testing.T) {
	c := scryfalltest.NewSearchServer(t)
	p1, err := c.SearchCards("bolt", "")
	if err != nil || len(p1.Cards) != 3 || p1.Total != 5 || p1.NextPage == "" {
		t.Fatalf("page 1 = %+v, %v", p1, err)
	}
	p2, err := c.SearchCards("bolt", p1.NextPage)
	if err != nil || len(p2.Cards) != 2 || p2.NextPage != "" {
		t.Fatalf("page 2 = %+v, %v", p2, err)
	}
	none, err := c.SearchCards("nothing", "")
	if err != nil || len(none.Cards) != 0 {
		t.Fatalf("no-match search should be empty without error: %+v, %v", none, err)
	}
	if _, err := c.SearchCards("bolt", "https://evil.example/cards/search"); err == nil {
		t.Fatal("next page on another host must be rejected")
	}
}
