package ui

import (
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

// TestToolsAndSettings: Tools and Settings are tree cards (Home, Tools, the
// collection, Settings), Tools lists the actions, and settings apply at once.
func TestToolsAndSettings(t *testing.T) {
	a := test.NewTempApp(t)
	// Like fyne.Do from the UI thread: queued, not run inside the current event.
	runOnUI = func(f func()) { go onUI(f) }
	t.Cleanup(func() { runOnUI = fyne.Do })
	w := test.NewTempWindow(t, nil)
	w.Resize(fyne.NewSize(1200, 900))
	store, _ := collection.OpenStore(filepath.Join(t.TempDir(), "c.json"), mtg.ID)
	store.AddCards([]collection.Card{
		held(collection.Card{ID: "a", Name: "Opt", Set: "XLN", SetName: "Ixalan", CollectorNumber: "65", ReleasedAt: "2017-09-29"}, "nonfoil", "", collection.Purchase{Quantity: 1}),
		held(collection.Card{ID: "b", Name: "Shock", Set: "M19", SetName: "Core Set 2019", CollectorNumber: "156", ReleasedAt: "2018-07-13"}, "nonfoil", "", collection.Purchase{Quantity: 1}),
	})
	ui := newTestApp(w, mtg.New(), store)
	onUI(func() { w.SetContent(ui.BuildLayout()) })

	var root []string
	onUI(func() { root = ui.tree.ChildUIDs("") })
	if want := []string{nodeHome, gameNode(mtg.ID), nodeTools, nodeSettings}; !equal(root, want) {
		t.Fatalf("tree root = %v, want %v", root, want)
	}

	onUI(func() { ui.tree.Select(nodeTools) })
	for _, label := range []string{"Import…", "Export collection…", "Bulk edit…", "Refresh prices now", "Load price history"} {
		var b *widget.Button
		onUI(func() { b = findButton(ui.rightPane, label) })
		if b == nil {
			t.Errorf("Tools page has no %q", label)
		}
	}

	onUI(func() { snapshot(t, w, "s1-tools.png") })

	onUI(func() { ui.tree.Select(nodeSettings) })
	waitFor(t, "settings page", func() bool { return ui.page == "settings" })
	selectByValue := func(current, choose string) {
		t.Helper()
		var sel *widget.Select
		waitFor(t, "menu showing "+current, func() bool {
			walk(ui.rightPane, func(o fyne.CanvasObject) {
				if s, ok := o.(*widget.Select); ok && s.Selected == current {
					sel = s
				}
			})
			return sel != nil
		})
		onUI(func() { sel.SetSelected(choose) })
	}

	selectByValue("Match system", "Dark")
	if waitFor(t, "theme saved", func() bool { return ui.settings.Get().Theme == "dark" }); false {
		t.Fatalf("theme setting = %q", ui.settings.Get().Theme)
	}
	waitFor(t, "dark theme", func() bool {
		return a.Settings().Theme().Color(theme.ColorNameBackground, theme.VariantLight) == theme.DefaultTheme().Color(theme.ColorNameBackground, theme.VariantDark)
	})

	time.Sleep(200 * time.Millisecond) // let the page redraw in the new theme
	onUI(func() { snapshot(t, w, "s2-settings-dark.png") })

	// The cards sit directly under the game, by name to start (Opt, Shock), then by
	// set, newest first (Shock from 2018 before Opt from 2017).
	if got := ui.tree.ChildUIDs(gameNode(mtg.ID)); !equal(got, []string{"card:mtg:a", "card:mtg:b"}) {
		t.Fatalf("cards by name = %v", got)
	}
	selectByValue("By name", "By set, newest first")
	waitFor(t, "cards by set", func() bool { return equal(ui.tree.ChildUIDs(gameNode(mtg.ID)), []string{"card:mtg:b", "card:mtg:a"}) })
}

func equal(a, b []string) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}
