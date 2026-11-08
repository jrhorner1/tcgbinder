package collection

import (
	"fmt"
	"path/filepath"
	"testing"
	"time"
)

// Benchmarks over collections like a real one: every card carries Magic's attributes,
// one to three purchases in two finishes, and 90 days of prices per finish.
//
//	go test -run '^$' -bench . -benchmem ./internal/collection

// benchCards makes n cards spread over 40 sets.
func benchCards(n int) []Card {
	cards := make([]Card, n)
	for i := range cards {
		c := Card{
			ID: fmt.Sprintf("%08x-0000-4000-8000-%012x", i, i), Name: fmt.Sprintf("Card Name %d", i),
			Set: fmt.Sprintf("S%02d", i%40), SetName: fmt.Sprintf("Set %d", i%40), CollectorNumber: fmt.Sprint(i/40 + 1),
			Rarity: []string{"common", "uncommon", "rare", "mythic"}[i%4], ReleasedAt: fmt.Sprintf("20%02d-06-01", 10+i%40/4),
			ImageURI: "https://cards.scryfall.io/normal/front/a/b/abcdef.jpg", Language: "en",
			TypeLine: "Creature — Elf Druid", RulesText: "{T}: Add {G}.\nFlavor that is a reasonable length for a card.",
			Finishes: []string{"nonfoil", "foil"}, Market: map[string]float64{"nonfoil": float64(i%500) / 10, "foil": float64(i%900) / 10},
			PricesUpdated: "2026-11-08",
		}
		c.SetAttr("oracle_id", fmt.Sprintf("o-%d", i))
		c.SetAttr("mana_cost", "{1}{G}")
		c.SetAttr("colors", []string{"G"})
		c.SetAttr("color_identity", []string{"G"})
		c.SetAttr("cmc", 2)
		c.SetAttr("multiverse_ids", []int{i})
		c.SetAttr("tcgplayer_id", i)
		c.SetAttr("set_id", "set-id")
		c.AddPurchase("nonfoil", ConditionNearMint, Purchase{Quantity: 1 + i%3, PricePaid: 0.25, AcquiredDate: fmt.Sprintf("2026-%02d-15", 1+i%10)})
		if i%3 == 0 {
			c.AddPurchase("foil", ConditionLightPlay, Purchase{Quantity: 1, PricePaid: 1.5, AcquiredDate: "2026-09-01"})
		}
		if i%5 == 0 {
			c.AddPurchase("nonfoil", ConditionNearMint, Purchase{Quantity: 2, PricePaid: 0.3, AcquiredDate: "2026-10-20"})
		}
		cards[i] = c
	}
	return cards
}

func benchStore(b *testing.B, n int) *Store {
	b.Helper()
	s, err := OpenStore(filepath.Join(b.TempDir(), "collection.json"), "mtg")
	if err != nil {
		b.Fatal(err)
	}
	if _, _, err := s.AddCards(benchCards(n)); err != nil {
		b.Fatal(err)
	}
	return s
}

func benchHistory(b *testing.B, cards []Card) *PriceHistory {
	b.Helper()
	h, _ := OpenPriceHistory(filepath.Join(b.TempDir(), "price-history.json"))
	start := time.Date(2026, 8, 10, 0, 0, 0, 0, time.UTC)
	for _, c := range cards {
		for d := 0; d < 90; d++ {
			date := start.AddDate(0, 0, d).Format("2006-01-02")
			for f, p := range c.Market {
				h.Set(c.ID, f, date, p*(1+float64(d%7)/100))
			}
		}
	}
	return h
}

var sizes = []int{500, 20000}

func BenchmarkLoad(b *testing.B) {
	for _, n := range sizes {
		b.Run(fmt.Sprint(n), func(b *testing.B) {
			path := benchStore(b, n).FilePath()
			b.ResetTimer()
			for b.Loop() {
				if _, err := OpenStore(path, "mtg"); err != nil {
					b.Fatal(err)
				}
			}
		})
	}
}

// BenchmarkEdit is one edit as the UIs make it: update a card, snapshot, save.
func BenchmarkEdit(b *testing.B) {
	for _, n := range sizes {
		b.Run(fmt.Sprint(n), func(b *testing.B) {
			s := benchStore(b, n)
			card := s.GetCollection()[0]
			b.ResetTimer()
			for b.Loop() {
				if _, _, err := s.SaveCard(card.ID, card); err != nil {
					b.Fatal(err)
				}
			}
		})
	}
}

func BenchmarkGetCollection(b *testing.B) {
	for _, n := range sizes {
		b.Run(fmt.Sprint(n), func(b *testing.B) {
			s := benchStore(b, n)
			for b.Loop() {
				_ = s.GetCollection()
			}
		})
	}
}

func BenchmarkComputeStats(b *testing.B) {
	for _, n := range sizes {
		b.Run(fmt.Sprint(n), func(b *testing.B) {
			cards := benchCards(n)
			for b.Loop() {
				_ = ComputeStats(cards, nil)
			}
		})
	}
}

// BenchmarkFilter is one keystroke in the search box: filter and sort by number.
func BenchmarkFilter(b *testing.B) {
	for _, n := range sizes {
		b.Run(fmt.Sprint(n), func(b *testing.B) {
			cards := benchCards(n)
			q := Query{Text: "name 1", SortBy: SortNumber}
			for b.Loop() {
				_ = ApplyQuery(cards, q)
			}
		})
	}
}

func BenchmarkMarketSeries(b *testing.B) {
	for _, n := range sizes {
		b.Run(fmt.Sprint(n), func(b *testing.B) {
			cards := benchCards(n)
			h := benchHistory(b, cards)
			from, to := time.Date(2026, 8, 10, 0, 0, 0, 0, time.UTC), time.Date(2026, 11, 8, 0, 0, 0, 0, time.UTC)
			b.ResetTimer()
			for b.Loop() {
				_ = h.MarketSeries(cards, from, to)
			}
		})
	}
}

func BenchmarkPriceHistoryLoadSave(b *testing.B) {
	for _, n := range sizes {
		b.Run(fmt.Sprint(n), func(b *testing.B) {
			h := benchHistory(b, benchCards(n))
			if err := h.Save(); err != nil {
				b.Fatal(err)
			}
			b.ResetTimer()
			for b.Loop() {
				h2, err := OpenPriceHistory(h.path)
				if err != nil {
					b.Fatal(err)
				}
				if err := h2.Save(); err != nil {
					b.Fatal(err)
				}
			}
		})
	}
}
