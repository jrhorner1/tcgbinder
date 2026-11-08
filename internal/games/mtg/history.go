package mtg

import (
	"fmt"
	"strings"

	"github.com/jrhorner1/mtgmanager/internal/collection"
)

func (g *Game) PriceHistoryInfo() string {
	return "the last 90 days of TCGplayer prices from MTGJSON (about a 50 MB download)"
}

// PriceHistory matches the cards' printings to MTGJSON through the per-set files and
// returns MTGJSON's daily prices for them, keyed by Scryfall ID.
func (g *Game) PriceHistory(cards []collection.Card, status func(string)) (map[string]map[string]map[string]float64, error) {
	setCodes, seenSet := []string{}, map[string]bool{}
	ids := map[string]bool{}
	for _, card := range cards {
		if code := strings.ToUpper(card.Set); code != "" && !seenSet[code] {
			seenSet[code] = true
			setCodes = append(setCodes, code)
		}
		ids[card.ID] = true
	}

	status(fmt.Sprintf("Matching %d sets with MTGJSON…", len(setCodes)))
	uuids, err := g.MTGJSON.UUIDs(setCodes)
	if err != nil {
		return nil, err
	}
	want, scryfallByUUID := map[string]bool{}, map[string]string{}
	for id := range ids {
		if u, ok := uuids[id]; ok {
			want[u] = true
			scryfallByUUID[u] = id
		}
	}
	if len(want) == 0 {
		return nil, fmt.Errorf("none of your cards were found on MTGJSON")
	}

	status("Downloading price history…")
	prices, err := g.MTGJSON.PriceHistory(want, func(read, total int64) {
		if total > 0 {
			status(fmt.Sprintf("Downloading price history… %d of %d MB", read>>20, total>>20))
		}
	})
	if err != nil {
		return nil, err
	}
	out := make(map[string]map[string]map[string]float64, len(prices))
	for u, dp := range prices {
		out[scryfallByUUID[u]] = dp
	}
	return out, nil
}
