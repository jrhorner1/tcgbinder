package scryfall

import (
	"os"
	"testing"
)

// TestLiveSearch hits the real Scryfall search; skipped unless MTG_LIVE=1.
func TestLiveSearch(t *testing.T) {
	if os.Getenv("MTG_LIVE") == "" {
		t.Skip("set MTG_LIVE=1 to query the live Scryfall API")
	}
	c := NewClient()
	for _, tc := range [][2]string{{"Lightning Bolt", ""}, {"Scapeshift", "m19"}, {"", "dom"}, {"zzzznotacard", ""}} {
		q, err := BuildSearchQuery(tc[0], tc[1])
		if err != nil {
			t.Fatal(err)
		}
		p, err := c.SearchCards(q, "")
		if err != nil {
			t.Fatalf("%q: %v", q, err)
		}
		first := ""
		if len(p.Cards) > 0 {
			first = p.Cards[0].Name + " " + p.Cards[0].Set + " #" + p.Cards[0].Number
		}
		t.Logf("%-28q total=%-4d page=%-3d more=%-5v first=%s", q, p.Total, len(p.Cards), p.NextPage != "", first)
		if p.NextPage != "" {
			p2, err := c.SearchCards(q, p.NextPage)
			if err != nil || len(p2.Cards) == 0 {
				t.Fatalf("%q page 2: %d cards, %v", q, len(p2.Cards), err)
			}
			t.Logf("   page 2 ok: %d cards", len(p2.Cards))
		}
	}
}
