package ui

import (
	"path/filepath"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"github.com/jrhorner1/mtgmanager/internal/collection"
	"github.com/jrhorner1/mtgmanager/internal/games/mtg"
)

// TestEditorFlow opens a card in the editor, changes its finish to one already owned,
// saves, and checks the entries merged and the editor moved to the merged entry.
func TestEditorFlow(t *testing.T) {
	a := test.NewTempApp(t)
	a.Settings().SetTheme(theme.DefaultTheme())
	runOnUI = onUI
	t.Cleanup(func() { runOnUI = fyne.Do })

	store, _ := collection.NewStore(filepath.Join(t.TempDir(), "c.json"))
	base := collection.Card{
		ID: "11111111-1111-1111-1111-111111111111", Name: "Dwarven Priest",
		Set: "M19", SetName: "Core Set 2019", Number: "11", Rarity: "common", Finishes: []string{"nonfoil", "foil"},
		TypeLine: "Creature — Dwarf Cleric", Text: "When Dwarven Priest enters the battlefield, you gain 1 life for each creature you control.",
		ReleasedAt: "2018-07-13", Market: map[string]float64{"nonfoil": 0.10, "foil": 0.40}, PricesUpdated: "2026-11-07",
		Finish: "nonfoil", Condition: collection.ConditionNearMint, Language: "en", Quantity: 3, PurchasePrice: 0.11, AcquiredDate: "2018-08-08",
	}
	base.SetAttr("mana_cost", "{3}{W}") // Magic-specific, shown by the game's Details
	foil := base
	foil.Finish, foil.Quantity = "foil", 1
	_, _, _ = store.AddCards([]collection.Card{base, foil})

	w := test.NewTempWindow(t, nil)
	w.Resize(fyne.NewSize(1300, 1000))
	ui := newTestApp(w, newMTG(), store)
	onUI(func() {
		w.SetContent(ui.BuildLayout())
		ui.selectCardNode(base)
	})
	snapshot(t, w, "9-editor.png")

	var save, revert *widget.Button
	var finish *widget.Select
	onUI(func() {
		pane := ui.rightPane
		save = findButton(pane, "Save Changes")
		revert = findButton(pane, "Revert")
		walk(pane, func(o fyne.CanvasObject) {
			if s, ok := o.(*widget.Select); ok && s.Selected == "nonfoil" {
				finish = s
			}
		})
	})
	if save == nil || revert == nil || finish == nil {
		t.Fatal("editor is missing controls")
	}
	if !save.Disabled() || !revert.Disabled() {
		t.Error("Save/Revert should be disabled until something changes")
	}
	onUI(func() { finish.SetSelected("foil") })
	if save.Disabled() {
		t.Fatal("Save should enable after an edit")
	}
	onUI(func() { test.Tap(save) })

	cards := store.GetCollection()
	if len(cards) != 1 || cards[0].Finish != "foil" || cards[0].Quantity != 4 {
		t.Fatalf("after save: %+v", cards)
	}
	var selected string
	onUI(func() { selected = ui.selected })
	if selected != cardNode(mtg.ID, cards[0]) {
		t.Errorf("tree selection = %q, want the merged entry", selected)
	}
	snapshot(t, w, "10-editor-after-merge.png")
}
