package collection

import (
	"fmt"
	"math"
	"os"
	"path/filepath"
	"sync"
	"testing"
	"time"
)

func day(s string) time.Time {
	t, _ := time.Parse("2006-01-02", s)
	return t
}

func TestMarketSeries(t *testing.T) {
	h, _ := LoadPriceHistory(filepath.Join(t.TempDir(), "ph.json"))
	h.Merge("shock", map[string]map[string]float64{
		"nonfoil": {"2026-11-01": 1.00, "2026-11-03": 2.00},
		"foil":    {"2026-11-01": 10.00},
	})
	h.Set("opt", "nonfoil", "2026-11-02", 0.50)

	cards := []Card{
		{ID: "shock", Finish: "nonfoil", Quantity: 2},                                         // undated: owned all along
		{ID: "opt", Finish: "nonfoil", Quantity: 4, AcquiredDate: "2026-11-02"},               // bought mid-range
		{ID: "new", Finish: "nonfoil", Quantity: 1, Market: map[string]float64{"nonfoil": 3}}, // no history: current price
	}
	got := h.MarketSeries(cards, day("2026-11-01"), day("2026-11-07"))
	want := []struct {
		date  string
		value float64
	}{
		// 11-01: shock 2×1.00 + new 3 (opt not owned yet)
		{"2026-11-01", 5.00},
		// 11-02: shock 2×1.00 (carried) + opt 4×0.50 + new 3
		{"2026-11-02", 7.00},
		// 11-03: shock 2×2.00 + opt 4×0.50 (carried) + new 3
		{"2026-11-03", 9.00},
		// 11-07: range end, prices carried forward from 11-03
		{"2026-11-07", 9.00},
	}
	if len(got) != len(want) {
		t.Fatalf("series = %+v", got)
	}
	for i, w := range want {
		if got[i].Date.Format("2006-01-02") != w.date || math.Abs(got[i].Value-w.value) > 1e-9 {
			t.Errorf("[%d] = %s %.2f, want %s %.2f", i, got[i].Date.Format("2006-01-02"), got[i].Value, w.date, w.value)
		}
	}

	// A range with no recorded prices inside it still spans start to end, carrying the
	// last known price forward.
	late := h.MarketSeries([]Card{{ID: "opt", Finish: "nonfoil", Quantity: 1}}, day("2026-11-03"), day("2026-11-07"))
	if len(late) != 2 || late[0].Value != 0.50 || late[1].Value != 0.50 ||
		!late[0].Date.Equal(day("2026-11-03")) || !late[1].Date.Equal(day("2026-11-07")) {
		t.Errorf("late range = %+v", late)
	}
	// A card whose history starts after a day uses its earliest price for that day.
	early := h.MarketSeries([]Card{{ID: "opt", Finish: "nonfoil", Quantity: 1}}, day("2026-11-01"), day("2026-11-01"))
	if len(early) != 1 || early[0].Value != 0.50 {
		t.Errorf("early range = %+v", early)
	}
	if !h.Start().Equal(day("2026-11-01")) {
		t.Errorf("start = %v", h.Start())
	}
}

func TestPriceHistoryRecordSaveLoad(t *testing.T) {
	path := filepath.Join(t.TempDir(), "ph.json")
	h, _ := LoadPriceHistory(path)
	h.RecordCurrent([]Card{{ID: "x", Market: map[string]float64{"nonfoil": 1.25, "foil": 4.00}}}, "2026-11-07")
	h.MarkBackfilled("2026-11-07")
	if err := h.Save(); err != nil {
		t.Fatal(err)
	}
	again, err := LoadPriceHistory(path)
	if err != nil {
		t.Fatal(err)
	}
	if again.Cards["x"]["nonfoil"]["2026-11-07"] != 1.25 || again.Cards["x"]["foil"]["2026-11-07"] != 4.00 || again.BackfilledOn() != "2026-11-07" {
		t.Fatalf("reloaded = %+v", again)
	}
}

// TestPriceHistoryKeepsEveryDay records a price per day for 400 days, saving and
// reloading from disk each day like the app does, and checks nothing was dropped —
// in particular nothing past MTGJSON's 90-day window.
func TestPriceHistoryKeepsEveryDay(t *testing.T) {
	path := filepath.Join(t.TempDir(), "ph.json")
	start := day("2025-10-01")
	for i := 0; i < 400; i++ {
		h, err := LoadPriceHistory(path)
		if err != nil {
			t.Fatal(err)
		}
		d := start.AddDate(0, 0, i).Format("2006-01-02")
		h.RecordCurrent([]Card{{ID: "x", Market: map[string]float64{"nonfoil": 1.00}}}, d)
		if err := h.Save(); err != nil {
			t.Fatal(err)
		}
	}
	h, _ := LoadPriceHistory(path)
	if n := len(h.Cards["x"]["nonfoil"]); n != 400 {
		t.Fatalf("kept %d of 400 days", n)
	}
	if !h.Start().Equal(start) {
		t.Fatalf("history starts %v, want %v", h.Start(), start)
	}
}

// TestBackfillAgainKeepsOlderDays: a later backfill covers a newer 90-day window;
// days only in the earlier one must stay, and prices the app recorded itself win.
func TestBackfillAgainKeepsOlderDays(t *testing.T) {
	h, _ := LoadPriceHistory(filepath.Join(t.TempDir(), "ph.json"))
	h.Merge("x", map[string]map[string]float64{"nonfoil": {"2026-01-01": 1, "2026-02-01": 2}})
	h.Set("x", "nonfoil", "2026-05-01", 5) // recorded from Scryfall by the app
	h.Merge("x", map[string]map[string]float64{"nonfoil": {"2026-04-01": 4, "2026-05-01": 99}})
	got := h.Cards["x"]["nonfoil"]
	if len(got) != 4 || got["2026-01-01"] != 1 || got["2026-02-01"] != 2 || got["2026-05-01"] != 5 {
		t.Fatalf("after second backfill: %v", got)
	}
}

// TestConcurrentSavesKeepEverything: the app records prices and runs a backfill at
// the same time; whichever save lands last must still contain every price.
func TestConcurrentSavesKeepEverything(t *testing.T) {
	path := filepath.Join(t.TempDir(), "ph.json")
	h, _ := LoadPriceHistory(path)
	var wg sync.WaitGroup
	for i := 0; i < 60; i++ {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			h.Set(fmt.Sprintf("card-%d", i), "nonfoil", "2026-11-07", float64(i))
			if err := h.Save(); err != nil {
				t.Error(err)
			}
		}(i)
	}
	wg.Wait()
	again, err := LoadPriceHistory(path)
	if err != nil {
		t.Fatal(err)
	}
	if n := again.Points(); n != 60 {
		t.Fatalf("file has %d of 60 prices", n)
	}
}

// TestSaveRefusesToShrink guards the invariant directly: no save may write fewer
// prices than the file held when loaded.
func TestSaveRefusesToShrink(t *testing.T) {
	path := filepath.Join(t.TempDir(), "ph.json")
	h, _ := LoadPriceHistory(path)
	h.Set("x", "nonfoil", "2026-11-06", 1)
	h.Set("x", "nonfoil", "2026-11-07", 1)
	if err := h.Save(); err != nil {
		t.Fatal(err)
	}
	before, _ := os.ReadFile(path)

	loaded, _ := LoadPriceHistory(path)
	delete(loaded.Cards["x"]["nonfoil"], "2026-11-06") // simulate a bug that drops data
	if err := loaded.Save(); err == nil {
		t.Fatal("a shrinking save should be refused")
	}
	after, _ := os.ReadFile(path)
	if string(before) != string(after) {
		t.Fatal("the file changed after a refused save")
	}
}

// TestUnreadableHistoryIsKept: a damaged file is set aside, never overwritten.
func TestUnreadableHistoryIsKept(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "price-history.json")
	garbage := []byte(`{"cards": {"x": {"nonfoil": {"2026-11-07": 1.0` + "\x00")
	os.WriteFile(path, garbage, 0o600)

	h, err := LoadPriceHistory(path)
	if err == nil || h == nil {
		t.Fatalf("want a fresh history and an explanation, got %v, %v", h, err)
	}
	matches, _ := filepath.Glob(path + ".unreadable-*")
	if len(matches) != 1 {
		t.Fatalf("backup files: %v", matches)
	}
	if kept, _ := os.ReadFile(matches[0]); string(kept) != string(garbage) {
		t.Fatal("the unreadable file's contents were not preserved")
	}
	h.Set("y", "nonfoil", "2026-11-08", 2)
	if err := h.Save(); err != nil {
		t.Fatal(err)
	}
	if kept, _ := os.ReadFile(matches[0]); string(kept) != string(garbage) {
		t.Fatal("saving the new history touched the set-aside file")
	}
}
