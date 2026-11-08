package web

import (
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"testing"

	"github.com/jrhorner1/tcgbinder/internal/collection"
	"github.com/jrhorner1/tcgbinder/internal/games/mtg"
	"github.com/jrhorner1/tcgbinder/internal/service"
)

// BenchmarkEndpoints times the API calls the browser makes most, for a collection of
// 20,000 cards.
//
//	go test -run '^$' -bench . -benchmem ./internal/web
func BenchmarkEndpoints(b *testing.B) {
	store, err := collection.OpenStore(filepath.Join(b.TempDir(), "collection.json"), mtg.ID)
	if err != nil {
		b.Fatal(err)
	}
	cards := make([]collection.Card, 20000)
	for i := range cards {
		c := collection.Card{ID: fmt.Sprintf("%08x-0000-4000-8000-%012x", i, i), Name: fmt.Sprintf("Card %d", i),
			Set: fmt.Sprintf("S%02d", i%40), SetName: "Set", CollectorNumber: fmt.Sprint(i/40 + 1), Rarity: "common", Language: "en",
			ReleasedAt: "2020-01-01", Finishes: []string{"nonfoil", "foil"}, Market: map[string]float64{"nonfoil": 0.5, "foil": 2}}
		c.SetAttr("mana_cost", "{G}")
		c.SetAttr("colors", []string{"G"})
		c.AddPurchase("nonfoil", collection.ConditionNearMint, collection.Purchase{Quantity: 2, PricePaid: 0.25, AcquiredDate: "2026-01-01"})
		cards[i] = c
	}
	if _, _, err := store.AddCards(cards); err != nil {
		b.Fatal(err)
	}
	srv := New("TCG Binder", "bench", nil, service.FromStore(mtg.New(), store))
	for _, path := range []string{"/api/games/mtg/cards", "/api/games/mtg/cards?q=card+1", "/api/games/mtg/home",
		"/api/games/mtg/card?key=" + cards[123].ID, "/api/search?q=card"} {
		b.Run(path, func(b *testing.B) {
			for b.Loop() {
				rec := httptest.NewRecorder()
				srv.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, path, nil))
				if rec.Code != 200 {
					b.Fatalf("%s: %d %s", path, rec.Code, rec.Body)
				}
				b.SetBytes(int64(rec.Body.Len()))
				io.Discard.Write(rec.Body.Bytes())
			}
		})
	}
}
