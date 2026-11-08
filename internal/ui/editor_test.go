package ui

import (
	"path/filepath"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"github.com/jrhorner1/tcgbinder/internal/collection"
	"github.com/jrhorner1/tcgbinder/internal/games/mtg"
)

// TestEditorFlow opens a card with two purchases, changes one purchase's finish to
// foil and adds another purchase, saves, and checks the copies.
func TestEditorFlow(t *testing.T) {
	a := test.NewTempApp(t)
	a.Settings().SetTheme(theme.DefaultTheme())
	runOnUI = onUI
	t.Cleanup(func() { runOnUI = fyne.Do })

	store, _ := collection.NewStore(filepath.Join(t.TempDir(), "c.json"))
	card := held(collection.Card{
		ID: "11111111-1111-1111-1111-111111111111", Name: "Dwarven Priest",
		Set: "M19", SetName: "Core Set 2019", CollectorNumber: "11", Rarity: "common", Finishes: []string{"nonfoil", "foil"},
		TypeLine: "Creature — Dwarf Cleric", RulesText: "When Dwarven Priest enters the battlefield, you gain 1 life for each creature you control.",
		ReleasedAt: "2018-07-13", Market: map[string]float64{"nonfoil": 0.10, "foil": 0.40}, PricesUpdated: "2026-11-07", Language: "en",
	}, "nonfoil", collection.ConditionNearMint, collection.Purchase{Quantity: 3, PricePaid: 0.10, AcquiredDate: "2018-08-08"})
	card = held(card, "nonfoil", collection.ConditionNearMint, collection.Purchase{Quantity: 1, PricePaid: 0.12, AcquiredDate: "2019-01-01"})
	card.SetAttr("mana_cost", "{3}{W}") // Magic-specific, shown by the game's Details
	_, _, _ = store.AddCards([]collection.Card{card})

	w := test.NewTempWindow(t, nil)
	w.Resize(fyne.NewSize(1300, 1000))
	ui := newTestApp(w, newMTG(), store)
	onUI(func() {
		w.SetContent(ui.BuildLayout())
		ui.selectCardNode(card)
	})
	snapshot(t, w, "9-editor.png")

	var save, revert, addPurchase *widget.Button
	var finishes []*widget.Select
	onUI(func() {
		pane := ui.rightPane
		save = findButton(pane, "Save Changes")
		revert = findButton(pane, "Revert")
		addPurchase = findButton(pane, "Add purchase")
		walk(pane, func(o fyne.CanvasObject) {
			if s, ok := o.(*widget.Select); ok && s.Selected == "Normal" { // one per purchase row
				finishes = append(finishes, s)
			}
		})
	})
	if save == nil || revert == nil || addPurchase == nil || len(finishes) != 2 {
		t.Fatalf("editor controls: save=%v revert=%v add=%v finish menus=%d", save != nil, revert != nil, addPurchase != nil, len(finishes))
	}
	if !save.Disabled() || !revert.Disabled() {
		t.Error("Save/Revert should be disabled until something changes")
	}
	// The second row is the $0.12 purchase (rows are oldest first): make it foil.
	onUI(func() { finishes[1].SetSelected("Foil") })
	if save.Disabled() {
		t.Fatal("Save should enable after an edit")
	}
	onUI(func() { test.Tap(addPurchase) }) // adds 1 nonfoil NM copy dated today
	onUI(func() { test.Tap(save) })

	cards := store.GetCollection()
	if len(cards) != 1 || cards[0].CopyCount() != 5 {
		t.Fatalf("after save: %+v", cards)
	}
	c := cards[0]
	if foil := c.Purchases("foil", collection.ConditionNearMint); len(foil) != 1 || foil[0].PricePaid != 0.12 || foil[0].AcquiredDate != "2019-01-01" {
		t.Fatalf("foil purchases = %+v", foil)
	}
	if nonfoil := c.Purchases("nonfoil", collection.ConditionNearMint); len(nonfoil) != 2 || nonfoil[0].Quantity != 3 {
		t.Fatalf("nonfoil purchases = %+v", nonfoil)
	}
	var selected string
	onUI(func() { selected = ui.selected })
	if selected != cardNode(mtg.ID, c) {
		t.Errorf("tree selection = %q, want the card", selected)
	}
	snapshot(t, w, "10-editor-after-save.png")
}
