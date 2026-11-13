package ui

import (
	"path/filepath"
	"strings"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/widget"
	"github.com/jrhorner1/tcgbinder/internal/collection"
	"github.com/jrhorner1/tcgbinder/internal/games/mtg"
)

// TestTickAndBulkEdit: filter the tree, Select all shown, untick one in the tree, and
// Bulk edit changes exactly the ticked cards, then clears the ticks.
func TestTickAndBulkEdit(t *testing.T) {
	test.NewTempApp(t)
	runOnUI = onUI
	t.Cleanup(func() { runOnUI = fyne.Do })
	w := test.NewTempWindow(t, nil)
	w.Resize(fyne.NewSize(1200, 900))
	store, _ := collection.OpenStore(filepath.Join(t.TempDir(), "c.json"), mtg.ID)
	card := func(id, name, set, rarity string) collection.Card {
		return held(collection.Card{ID: id, Name: name, Set: set, SetName: set, CollectorNumber: "1", Rarity: rarity,
			Finishes: []string{"nonfoil"}}, "nonfoil", "", collection.Purchase{Quantity: 1})
	}
	store.AddCards([]collection.Card{
		card("a", "Opt", "XLN", "common"), card("b", "Shock", "M19", "common"),
		card("c", "Scapeshift", "M19", "rare"), card("d", "Bolt", "M11", "common"),
	})
	ui := newTestApp(w, mtg.New(), store)
	onUI(func() { w.SetContent(ui.BuildLayout()) })

	var bar string
	readBar := func() string { onUI(func() { bar = ui.selection.count.Text }); return bar }
	if !strings.HasPrefix(readBar(), "Tick cards") || !ui.selection.bulkEdit.Disabled() {
		t.Fatalf("nothing ticked: bar %q, bulk edit enabled=%v", bar, !ui.selection.bulkEdit.Disabled())
	}

	// Filter to the commons, tick them all, then untick Bolt in the tree.
	onUI(func() {
		ui.view.query.Rarity = "common"
		ui.applyFilter()
		test.Tap(ui.selection.selectAll)
	})
	if readBar() != "3 ticked" {
		t.Fatalf("after Select all shown: %q", bar)
	}
	onUI(func() { snapshot(t, w, "b1-ticked.png") })
	onUI(func() {
		var boltCheck *widget.Check
		walk(ui.tree, func(o fyne.CanvasObject) {
			if row, ok := o.(*fyne.Container); ok && len(row.Objects) == 3 {
				if l, ok := row.Objects[0].(*widget.Label); ok && l.Text == "Bolt" {
					boltCheck = row.Objects[1].(*fyne.Container).Objects[1].(*widget.Check)
				}
			}
		})
		if boltCheck == nil || !boltCheck.Checked {
			t.Fatal("Bolt's row should show a ticked checkbox")
		}
		test.Tap(boltCheck)
		ui.view.query.Rarity = "rare" // the ticked commons are now hidden
		ui.applyFilter()
	})
	if readBar() != "2 ticked (2 hidden by filters)" {
		t.Fatalf("with the commons filtered out: %q", bar)
	}

	// Bulk edit the two ticked cards to heavily played.
	onUI(func() { test.Tap(ui.selection.bulkEdit) })
	var top fyne.CanvasObject
	onUI(func() { top = w.Canvas().Overlays().Top() })
	if top == nil {
		t.Fatal("Bulk edit didn't open a dialog")
	}
	onUI(func() {
		walk(top, func(o fyne.CanvasObject) {
			if s, ok := o.(*widget.Select); ok && strings.Contains(strings.Join(s.Options, ","), "HP — Heavily Played") {
				s.SetSelected("HP — Heavily Played")
			}
		})
		snapshot(t, w, "b2-bulk-edit.png")
		test.Tap(findButton(top, "Apply"))
	})
	onUI(func() { test.Tap(findButton(w.Canvas().Overlays().Top(), "Yes")) }) // confirm
	for _, c := range store.GetCollection() {
		want := collection.ConditionNearMint
		if c.ID == "a" || c.ID == "b" {
			want = collection.ConditionHeavyPlay
		}
		if c.ConditionCopyCount("nonfoil", want) != 1 {
			t.Errorf("%s: copies = %+v, want %s", c.Name, c.Copies, want)
		}
	}
	if readBar() != "Tick cards to bulk edit them" {
		t.Fatalf("ticks should clear after the edit: %q", bar)
	}
}
