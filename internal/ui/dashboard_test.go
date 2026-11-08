package ui

import (
	"os"
	"path/filepath"
	"testing"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/theme"
	"github.com/jrhorner1/mtgmanager/internal/collection"
	"github.com/jrhorner1/mtgmanager/internal/games/mtg"
)

// TestDashboardSnapshots imports a real export against live Scryfall into a temporary
// collection and saves screenshots of Home, a set page and a card. Skipped unless
// MTG_IMPORT_FILE and MTG_SNAPSHOT_DIR are set.
func TestDashboardSnapshots(t *testing.T) {
	csv, dir := os.Getenv("MTG_IMPORT_FILE"), os.Getenv("MTG_SNAPSHOT_DIR")
	if csv == "" || dir == "" {
		t.Skip("set MTG_IMPORT_FILE and MTG_SNAPSHOT_DIR")
	}
	data, err := os.ReadFile(csv)
	if err != nil {
		t.Fatal(err)
	}
	_ = data
	store, _ := collection.OpenStore(filepath.Join(t.TempDir(), "c.json"), mtg.ID)
	g := newMTG()
	sets := liveImport(t, g, store, csv)

	a := test.NewTempApp(t)
	a.Settings().SetTheme(theme.DefaultTheme())
	runOnUI = onUI
	t.Cleanup(func() { runOnUI = fyne.Do })

	w := test.NewTempWindow(t, nil)
	w.Resize(fyne.NewSize(1300, 1500))
	ui := newTestApp(w, g, store)
	onUI(func() {
		w.SetContent(ui.BuildLayout())
		ui.g.sets = sets
		ui.RefreshDataIndex()
	})
	snapshot(t, w, "6-home.png")

	onUI(func() {
		ui.tree.OpenBranch(setNode(mtg.ID, "M19"))
		ui.tree.Select(setNode(mtg.ID, "M19"))
	})
	snapshot(t, w, "7-set.png")

	var first string
	onUI(func() {
		first = ui.g.setCards["M19"][len(ui.g.setCards["M19"])-1] // a rare near the end of the set
		ui.tree.Select(first)
	})
	time.Sleep(2 * time.Second) // card image download
	snapshot(t, w, "8-card.png")
}
