package ui

import (
	"os"
	"path/filepath"
	"testing"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"github.com/jrhorner1/tcgbinder/internal/collection"
	"github.com/jrhorner1/tcgbinder/internal/games/mtg"
)

// TestPhoneLayout drives the phone layout at iPhone size: it opens on the dashboard,
// Back shows the collection list, and tapping a set and a card opens their pages.
// With MTG_IMPORT_FILE set it uses that export (live Scryfall) instead of sample data;
// TCGBINDER_SNAPSHOT_DIR saves screenshots.
func TestPhoneLayout(t *testing.T) {
	a := test.NewTempApp(t)
	a.Settings().SetTheme(theme.DefaultTheme())
	runOnUI = onUI
	isMobile = func() bool { return true }
	t.Cleanup(func() {
		runOnUI = fyne.Do
		isMobile = func() bool { return fyne.CurrentDevice().IsMobile() }
	})

	store, _ := collection.OpenStore(filepath.Join(t.TempDir(), "c.json"), mtg.ID)
	g := newMTG()
	var sets map[string]collection.SetInfo
	if csv := os.Getenv("MTG_IMPORT_FILE"); csv != "" {
		sets = liveImport(t, g, store, csv)
	} else {
		_, _, _ = store.AddCards([]collection.Card{
			held(collection.Card{ID: "a", Name: "Scapeshift", Set: "M19", SetName: "Core Set 2019", CollectorNumber: "201", Rarity: "mythic",
				ReleasedAt: "2018-07-13", Finishes: []string{"nonfoil", "foil"}, Market: map[string]float64{"nonfoil": 41.88}, PricesUpdated: "2026-11-07"},
				"nonfoil", "", collection.Purchase{Quantity: 1, PricePaid: 30, AcquiredDate: "2018-08-08"}),
			held(collection.Card{ID: "b", Name: "Opt", Set: "DOM", SetName: "Dominaria", CollectorNumber: "60", Rarity: "common",
				ReleasedAt: "2018-04-27", Finishes: []string{"nonfoil", "foil"}, Market: map[string]float64{"nonfoil": 0.10, "foil": 0.40}, PricesUpdated: "2026-11-07"},
				"foil", "", collection.Purchase{Quantity: 4, PricePaid: 0.25, AcquiredDate: "2018-05-01"}),
		})
	}

	w := test.NewTempWindow(t, nil)
	w.Resize(fyne.NewSize(390, 844))
	ui := newTestApp(w, g, store)
	onUI(func() {
		w.SetContent(ui.BuildLayout())
		ui.g.sets = sets
		ui.RefreshDataIndex()
	})

	showing := func() fyne.CanvasObject {
		var o fyne.CanvasObject
		onUI(func() { o = ui.mobileRoot.Objects[0] })
		return o
	}
	if showing() != ui.detailView || ui.page != "home" {
		t.Fatal("phone layout should open on the dashboard")
	}
	onUI(func() { snapshot(t, w, "p1-home.png") })

	var back *widget.Button
	onUI(func() { back = findButton(ui.detailView, "Back") })
	if back == nil {
		t.Fatal("no Back button on the page screen")
	}
	onUI(func() { test.Tap(back) })
	if showing() != ui.listView {
		t.Fatal("Back should return to the collection list")
	}
	onUI(func() { snapshot(t, w, "p2-list.png") })

	set := "M19"
	onUI(func() { ui.openSet(set) }) // as from Home's Set Completion
	if showing() != ui.detailView || ui.page != "set:"+set {
		t.Fatalf("opening a set should show its page, page=%q", ui.page)
	}
	onUI(func() { snapshot(t, w, "p3-set.png") })

	onUI(func() { test.Tap(back) })
	onUI(func() { ui.tree.Select(ui.g.cardIDs[0]) })
	if showing() != ui.detailView || ui.page != "card" {
		t.Fatalf("tapping a card should open the editor, page=%q", ui.page)
	}
	time.Sleep(1500 * time.Millisecond) // image download when using live data
	onUI(func() { snapshot(t, w, "p4-card.png") })

	onUI(func() { test.Tap(back) })
	onUI(func() { snapshot(t, w, "p0-list.png") })
	for _, node := range []string{nodeTools, nodeSettings} {
		var page string
		onUI(func() { ui.tree.Select(node); page = ui.page })
		if showing() != ui.detailView || page != node {
			t.Fatalf("tapping %s should open its page, page=%q", node, page)
		}
		onUI(func() { snapshot(t, w, "p6-"+node+".png") })
		onUI(func() { test.Tap(back) })
	}

	onUI(func() { ui.showAddDialog() })
	onUI(func() { snapshot(t, w, "p5-add.png") })
}
