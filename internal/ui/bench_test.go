package ui

import (
	"fmt"
	"path/filepath"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/test"
	"github.com/jrhorner1/tcgbinder/internal/collection"
	"github.com/jrhorner1/tcgbinder/internal/games/mtg"
)

// BenchmarkSearchKeystroke times what one keystroke in the collection search box
// does with 20,000 cards: filter, regroup the tree, refresh it.
//
//	go test -run '^$' -bench . -benchmem ./internal/ui
func BenchmarkSearchKeystroke(b *testing.B) {
	test.NewTempApp(b)
	w := test.NewTempWindow(b, nil)
	w.Resize(fyne.NewSize(1200, 800))
	store, _ := collection.OpenStore(filepath.Join(b.TempDir(), "c.json"), mtg.ID)
	cards := make([]collection.Card, 20000)
	for i := range cards {
		c := collection.Card{ID: fmt.Sprintf("id-%d", i), Name: fmt.Sprintf("Card %d", i), Set: fmt.Sprintf("S%02d", i%40),
			SetName: "Set", CollectorNumber: fmt.Sprint(i/40 + 1), Rarity: "common", Finishes: []string{"nonfoil"},
			Market: map[string]float64{"nonfoil": 0.5}}
		c.AddPurchase("nonfoil", "", collection.Purchase{Quantity: 1})
		cards[i] = c
	}
	store.AddCards(cards)
	ui := newTestApp(w, mtg.New(), store)
	w.SetContent(ui.BuildLayout())
	b.ResetTimer()
	for i := 0; b.Loop(); i++ {
		ui.view.query.Text = []string{"card 1", "card 12", "card 123", ""}[i%4]
		ui.applyFilter()
	}
}
