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
	"github.com/jrhorner1/tcgbinder/internal/games/mtg/scryfall/scryfalltest"
)

// TestAddDialogFlow drives the Add dialog against a fake Scryfall: search, page,
// pick a non-first printing, narrow by set, and add. Set TCGBINDER_SNAPSHOT_DIR to save
// screenshots of each step.
func TestAddDialogFlow(t *testing.T) {
	a := test.NewTempApp(t)
	a.Settings().SetTheme(theme.DefaultTheme())
	runOnUI = onUI
	t.Cleanup(func() { runOnUI = fyne.Do })
	g := &mtg.Game{Scryfall: scryfalltest.NewSearchServer(t)} // offline catalog

	store, err := collection.NewStore(filepath.Join(t.TempDir(), "c.json"))
	if err != nil {
		t.Fatal(err)
	}
	w := test.NewTempWindow(t, nil)
	w.Resize(fyne.NewSize(1100, 760))
	ui := newTestApp(w, g, store)
	w.SetContent(ui.BuildLayout())
	snapshot(t, w, "1-main.png")

	addBtn := findButton(w.Content(), "Add")
	if addBtn == nil {
		t.Fatal("no Add button in main layout")
	}
	onUI(func() { test.Tap(addBtn) })
	var dlg fyne.CanvasObject
	onUI(func() { dlg = w.Canvas().Overlays().Top() })
	if dlg == nil {
		t.Fatal("Add dialog did not open")
	}

	var (
		name, set             *widget.Entry
		search, confirm, more *widget.Button
		results               *widget.List
	)
	onUI(func() {
		name = findEntry(dlg, "Card name")
		set = findEntry(dlg, "Set")
		search = findButton(dlg, "Search")
		confirm = findButton(dlg, "Add to Collection")
		more = findButton(dlg, "Load more results")
		results = findList(dlg, nil)
	})
	if name == nil || set == nil || search == nil || confirm == nil || results == nil {
		t.Fatal("dialog is missing a control")
	}
	if confirm.Disabled() == false {
		t.Error("Add to Collection should be disabled until a printing is selected")
	}

	onUI(func() { name.SetText("bolt"); test.Tap(search) })
	waitFor(t, "first page", func() bool { return results.Length() == 3 && !more.Hidden })
	onUI(func() { test.Tap(more) })
	waitFor(t, "second page", func() bool { return results.Length() == 5 && more.Hidden })

	onUI(func() { results.Select(3) }) // choose the 4th printing, not the first
	waitFor(t, "selection", func() bool { return !confirm.Disabled() })
	snapshot(t, w, "2-add-results.png")

	onUI(func() { set.SetText("m11"); test.Tap(search) })
	waitFor(t, "set-filtered results", func() bool { return results.Length() == 1 })
	onUI(func() { results.Select(0) })
	snapshot(t, w, "3-add-set-filter.png")

	onUI(func() { test.Tap(confirm) })
	cards := store.GetCollection()
	if len(cards) != 1 || cards[0].Set != "M11" || cards[0].CopyCount() != 1 {
		t.Fatalf("collection after add = %+v", cards)
	}
	snapshot(t, w, "4-after-add.png")
}

// held gives c copies in one finish and condition (test fixtures).
func held(c collection.Card, finish string, cond collection.Condition, l collection.Purchase) collection.Card {
	c.AddPurchase(finish, cond, l)
	return c
}
