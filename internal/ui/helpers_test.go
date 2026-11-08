package ui

import (
	"image/png"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/widget"
	"github.com/jrhorner1/tcgbinder/internal/collection"
	"github.com/jrhorner1/tcgbinder/internal/game"
	"github.com/jrhorner1/tcgbinder/internal/games/mtg"
	"github.com/jrhorner1/tcgbinder/internal/service"
	"github.com/jrhorner1/tcgbinder/internal/settings"
)

// walk visits every object in a canvas object tree.
func walk(o fyne.CanvasObject, visit func(fyne.CanvasObject)) {
	visit(o)
	switch x := o.(type) {
	case *fyne.Container:
		for _, c := range x.Objects {
			walk(c, visit)
		}
	case fyne.Widget:
		r := test.WidgetRenderer(x) // cached renderer; do not destroy, the widget stays live
		for _, c := range r.Objects() {
			walk(c, visit)
		}
	}
}

func findEntry(root fyne.CanvasObject, placeholder string) (e *widget.Entry) {
	walk(root, func(o fyne.CanvasObject) {
		if x, ok := o.(*widget.Entry); ok && strings.HasPrefix(x.PlaceHolder, placeholder) {
			e = x
		}
	})
	return e
}

func findButton(root fyne.CanvasObject, text string) (b *widget.Button) {
	walk(root, func(o fyne.CanvasObject) {
		if x, ok := o.(*widget.Button); ok && x.Text == text {
			b = x
		}
	})
	return b
}

func findList(root fyne.CanvasObject, skip *widget.List) (l *widget.List) {
	walk(root, func(o fyne.CanvasObject) {
		if x, ok := o.(*widget.List); ok && x != skip {
			l = x
		}
	})
	return l
}

// onUI runs f on the Fyne UI goroutine, like real input events, so the test doesn't
// race with fyne.Do callbacks from the dialog's background searches.
func onUI(f func()) {
	uiMu.Lock()
	defer uiMu.Unlock()
	f()
}

var uiMu sync.Mutex

func waitFor(t *testing.T, what string, cond func() bool) {
	t.Helper()
	deadline := time.Now().Add(5 * time.Second)
	for {
		var ok bool
		onUI(func() { ok = cond() })
		if ok {
			break
		}
		if time.Now().After(deadline) {
			t.Fatalf("timed out waiting for %s", what)
		}
		time.Sleep(20 * time.Millisecond)
	}
}

func snapshot(t *testing.T, w fyne.Window, name string) {
	dir := os.Getenv("TCGBINDER_SNAPSHOT_DIR")
	if dir == "" {
		return
	}
	f, err := os.Create(filepath.Join(dir, name))
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	if err := png.Encode(f, w.Canvas().Capture()); err != nil {
		t.Fatal(err)
	}
}

// newMTG returns the Magic game backed by the live services; tests swap in fake
// clients where they need to stay offline.
func newMTG() *mtg.Game { return mtg.New() }

// newTestApp builds the UI for one game and store.
func newTestApp(w fyne.Window, g game.Game, store *collection.Store) *App {
	return New(w, settings.Memory(), service.FromStore(g, store))
}

// liveImport imports a real export into store through the Magic game (live Scryfall).
func liveImport(t *testing.T, g *mtg.Game, store *collection.Store, csvPath string) map[string]collection.SetInfo {
	t.Helper()
	data, err := os.ReadFile(csvPath)
	if err != nil {
		t.Fatal(err)
	}
	cards, _, err := g.Import(data, nil)
	if err != nil {
		t.Fatal(err)
	}
	if _, _, err := store.AddCards(cards); err != nil {
		t.Fatal(err)
	}
	if err := g.Open(t.TempDir()); err != nil {
		t.Fatal(err)
	}
	sets, _ := g.Sets()
	return sets
}
