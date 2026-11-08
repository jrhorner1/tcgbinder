package ui

import (
	"path/filepath"
	"testing"

	"fyne.io/fyne/v2/test"
	"github.com/jrhorner1/mtgmanager/internal/collection"
	"github.com/jrhorner1/mtgmanager/internal/games/mtg"
)

func TestImportButtonLabel(t *testing.T) {
	test.NewTempApp(t)
	w := test.NewTempWindow(t, nil)
	store, _ := collection.OpenStore(filepath.Join(t.TempDir(), "c.json"), mtg.ID)
	if b := newTestApp(w, mtg.New(), store).ImportButton(w); b.Text != "Import" {
		t.Fatalf("button text %q", b.Text)
	}
}
