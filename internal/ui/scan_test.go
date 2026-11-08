package ui

import (
	"path/filepath"
	"testing"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/widget"
	"github.com/jrhorner1/tcgbinder/internal/collection"
	"github.com/jrhorner1/tcgbinder/internal/game"
	"github.com/jrhorner1/tcgbinder/internal/games/mtg"
	"github.com/jrhorner1/tcgbinder/internal/scan"
	"github.com/jrhorner1/tcgbinder/internal/scan/apple"
)

// scanGame is Magic with identification faked, so the test needs no network.
type scanGame struct {
	*mtg.Game
	res game.ScanResult
}

func (g scanGame) ScanKey(r scan.Reading) (string, bool)          { return r.Text(), len(r.Lines) > 0 }
func (g scanGame) Identify(scan.Reading) (game.ScanResult, error) { return g.res, nil }

// TestScanFlow: Scan opens the camera, which captures once two frames agree; the
// result offers the printings found, and "Add & scan next" adds the purchase and
// opens the camera again.
func TestScanFlow(t *testing.T) {
	test.NewTempApp(t)
	runOnUI = func(f func()) { go onUI(f) }
	cameraScanAvailable = func() bool { return true }
	scans := 0
	var verdicts []apple.Frame
	startCameraScan = func(photosOnly bool, judge func(scan.Reading) apple.Frame, done apple.ScanFunc) {
		scans++
		if scans > 1 { // the second scan (after "Add & scan next") is closed
			done(scan.Reading{}, apple.ErrCancelled)
			return
		}
		blank := scan.Reading{}
		shock := scan.Reading{CardFound: true, Lines: []scan.Line{{Text: "Shock"}}}
		for _, r := range []scan.Reading{blank, shock, shock} {
			verdicts = append(verdicts, judge(r))
		}
		done(shock, nil)
	}
	t.Cleanup(func() {
		runOnUI, cameraScanAvailable, startCameraScan = fyne.Do, apple.ScanAvailable, apple.Scan
	})

	shock := func(set, num string) collection.Card {
		return collection.Card{ID: "id-" + set, Name: "Shock", Set: set, SetName: set + " Edition", CollectorNumber: num, Rarity: "common",
			Finishes: []string{"nonfoil", "foil"}, Market: map[string]float64{"nonfoil": 0.2, "foil": 1.1}}
	}
	g := scanGame{Game: mtg.New(), res: game.ScanResult{Read: "Shock · M19 · 156", Name: "Shock", Exact: true,
		Candidates: []collection.Card{shock("M19", "156"), shock("2XM", "141")}}}
	w := test.NewTempWindow(t, nil)
	w.Resize(fyne.NewSize(1200, 900))
	store, _ := collection.OpenStore(filepath.Join(t.TempDir(), "c.json"), mtg.ID)
	ui := newTestApp(w, g, store)
	onUI(func() { w.SetContent(ui.BuildLayout()) })

	var scanButton *widget.Button
	onUI(func() { scanButton = findButton(w.Content(), "Scan") })
	if scanButton == nil {
		t.Fatal("no Scan button")
	}
	onUI(func() { test.Tap(scanButton) })
	if want := []apple.Frame{apple.FrameNothing, apple.FramePartial, apple.FrameAccept}; !equalFrames(verdicts, want) {
		t.Fatalf("frame verdicts = %v, want %v", verdicts, want)
	}
	var addNext *widget.Button
	waitFor(t, "scan result", func() bool { addNext = findButton(ui.rightPane, "Add & scan next"); return addNext != nil })
	time.Sleep(100 * time.Millisecond)
	onUI(func() { snapshot(t, w, "scan-result.png") })

	onUI(func() {
		var finish *widget.Select
		walk(ui.rightPane, func(o fyne.CanvasObject) {
			if s, ok := o.(*widget.Select); ok && s.Selected == "Normal" {
				finish = s
			}
		})
		finish.SetSelected("Foil")
		findEntry(ui.rightPane, "price paid").SetText("1.25")
		test.Tap(addNext)
	})
	waitFor(t, "second scan", func() bool { return scans == 2 })
	cards := store.GetCollection()
	if len(cards) != 1 || cards[0].ID != "id-M19" || cards[0].FinishCopyCount("foil") != 1 || cards[0].PurchaseRows()[0].PricePaid != 1.25 {
		t.Fatalf("collection after scan = %+v", cards)
	}
}

func equalFrames(a, b []apple.Frame) bool {
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
