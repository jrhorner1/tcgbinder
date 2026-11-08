package collection

import (
	"math"
	"path/filepath"
	"testing"
	"time"
)

func TestMarketPriceByFinish(t *testing.T) {
	p := map[string]float64{"nonfoil": 1, "foil": 3.5}
	for finish, want := range map[string]float64{"nonfoil": 1, "foil": 3.5} {
		got, ok := MarketPrice(Card{Market: p}, finish)
		if !ok || got != want {
			t.Errorf("%s: got %v %v, want %v", finish, got, ok, want)
		}
	}
	if _, ok := MarketPrice(Card{Market: map[string]float64{"nonfoil": 1}}, "foil"); ok {
		t.Error("foil with no foil price should be unpriced")
	}
	if _, ok := MarketPrice(Card{}, "nonfoil"); ok {
		t.Error("no prices should be unpriced")
	}
}

func TestComputeStats(t *testing.T) {
	pinToday(t, "2026-11-07")
	shock := Card{ID: "1", Name: "Shock", Set: "M19", SetName: "Core Set 2019", CollectorNumber: "156", ReleasedAt: "2018-07-13",
		Market: map[string]float64{"nonfoil": 0.20, "foil": 2.00}}
	shock = held(shock, "nonfoil", "", Purchase{Quantity: 3, PricePaid: 0.10, AcquiredDate: "2018-08-08"})
	shock = held(shock, "foil", "", Purchase{Quantity: 1, PricePaid: 1.00, AcquiredDate: "2019-01-01"})
	cards := []Card{
		shock,
		held(Card{ID: "2", Name: "Opt", Set: "DOM", SetName: "Dominaria", CollectorNumber: "60", ReleasedAt: "2018-04-27"},
			"nonfoil", "", Purchase{Quantity: 2, AcquiredDate: "2018-08-08"}),
	}
	sets := map[string]SetInfo{"M19": {Code: "m19", Name: "Core Set 2019", CardCount: 280, ReleasedAt: "2018-07-13"}}
	st := ComputeStats(cards, sets)

	if st.Cards != 6 || st.UniqueCards != 2 || st.Sets != 2 {
		t.Fatalf("counts = %d/%d/%d", st.Cards, st.UniqueCards, st.Sets)
	}
	if math.Abs(st.PricePaid-1.30) > 1e-9 || st.PricePaidUnknown != 2 {
		t.Errorf("cost = %v unknown %d", st.PricePaid, st.PricePaidUnknown)
	}
	if math.Abs(st.MarketValue-2.60) > 1e-9 || st.Unpriced != 2 {
		t.Errorf("market = %v unknown %d", st.MarketValue, st.Unpriced)
	}
	if st.BySet[0].Code != "M19" || st.BySet[1].Code != "DOM" {
		t.Errorf("sets should be newest first: %v, %v", st.BySet[0].Code, st.BySet[1].Code)
	}
	m19 := st.BySet[0]
	if m19.Collected != 1 || m19.SetSize != 280 || math.Abs(m19.Completion()-1.0/280) > 1e-9 {
		t.Errorf("M19 completion: %+v", m19)
	}
	if st.BySet[1].Completion() != -1 {
		t.Error("unknown set size should report -1 completion")
	}
	if len(st.TopCards) != 1 || st.TopCards[0].Total != 2.60 || st.TopCards[0].Card.Name != "Shock" {
		t.Errorf("top cards = %+v", st.TopCards)
	}
	// Both histories step by acquired date and run through today.
	wantDates := []string{"2018-08-08", "2019-01-01", "2026-11-07"}
	check := func(name string, got []TimePoint, want []float64) {
		t.Helper()
		if len(got) != len(want) {
			t.Fatalf("%s = %+v", name, got)
		}
		for i := range want {
			if got[i].Date.Format("2006-01-02") != wantDates[i] || math.Abs(got[i].Value-want[i]) > 1e-9 {
				t.Errorf("%s[%d] = %v %v, want %s %v", name, i, got[i].Date.Format("2006-01-02"), got[i].Value, wantDates[i], want[i])
			}
		}
	}
	check("cost history", st.PricePaidHistory, []float64{0.30, 1.30, 1.30})
	check("value history", st.ValueHistory, []float64{0.60, 2.60, 2.60})
	if last := st.ValueHistory[len(st.ValueHistory)-1].Value; math.Abs(last-st.MarketValue) > 1e-9 {
		t.Errorf("value history should end at the market total %v, got %v", st.MarketValue, last)
	}
}

func TestHistoryIncludesUndatedCards(t *testing.T) {
	pinToday(t, "2026-11-07")
	st := ComputeStats([]Card{
		held(Card{ID: "a", Market: map[string]float64{"nonfoil": 5}}, "nonfoil", "", Purchase{Quantity: 1, PricePaid: 2}),                             // no date
		held(Card{ID: "b", Market: map[string]float64{"nonfoil": 3}}, "nonfoil", "", Purchase{Quantity: 1, PricePaid: 1, AcquiredDate: "2020-01-01"}), // dated
	}, nil)
	if len(st.ValueHistory) != 2 || st.ValueHistory[0].Value != 8 || st.ValueHistory[1].Value != 8 {
		t.Errorf("undated value should count from the first point: %+v", st.ValueHistory)
	}
	only := ComputeStats([]Card{held(Card{ID: "a", Market: map[string]float64{"nonfoil": 1}}, "nonfoil", "", Purchase{Quantity: 2})}, nil)
	if len(only.ValueHistory) != 1 || only.ValueHistory[0].Value != 2 {
		t.Errorf("all-undated collection should be one point today: %+v", only.ValueHistory)
	}
}

func pinToday(t *testing.T, d string) {
	t.Helper()
	day, _ := time.Parse("2006-01-02", d)
	orig := today
	today = func() time.Time { return day }
	t.Cleanup(func() { today = orig })
}

func TestUpsertSnapshot(t *testing.T) {
	h := upsertSnapshot(nil, ValueSnapshot{Date: "2026-11-07", MarketValue: 1})
	h = upsertSnapshot(h, ValueSnapshot{Date: "2026-11-05", MarketValue: 2})
	h = upsertSnapshot(h, ValueSnapshot{Date: "2026-11-07", MarketValue: 3})
	if len(h) != 2 || h[0].Date != "2026-11-05" || h[1].MarketValue != 3 {
		t.Fatalf("history = %+v", h)
	}
}

func TestStoreRecordsDailySnapshot(t *testing.T) {
	store, _ := NewStore(filepath.Join(t.TempDir(), "c.json"))
	_, _, _ = store.AddCards([]Card{held(Card{ID: "a", Market: map[string]float64{"nonfoil": 3}}, "nonfoil", "", Purchase{Quantity: 2, PricePaid: 1})})
	_, _, _ = store.AddCards([]Card{held(Card{ID: "b", Market: map[string]float64{"nonfoil": 4}}, "nonfoil", "", Purchase{Quantity: 1})})
	h := store.ValueHistory()
	if len(h) != 1 || h[0].Date != time.Now().Format("2006-01-02") || h[0].MarketValue != 10 || h[0].PricePaid != 2 || h[0].Cards != 3 {
		t.Fatalf("history = %+v", h)
	}
	reloaded, _ := NewStore(store.FilePath())
	if len(reloaded.ValueHistory()) != 1 {
		t.Fatal("history should persist")
	}
}

// TestValueHistoryNeverTruncated: daily value snapshots accumulate indefinitely.
func TestValueHistoryNeverTruncated(t *testing.T) {
	var h []ValueSnapshot
	start := day("2025-01-01")
	for i := 0; i < 500; i++ {
		h = upsertSnapshot(h, ValueSnapshot{Date: start.AddDate(0, 0, i).Format("2006-01-02"), MarketValue: float64(i)})
	}
	h = upsertSnapshot(h, ValueSnapshot{Date: "2025-01-01", MarketValue: 7}) // same day replaces, never removes
	if len(h) != 500 || h[0].MarketValue != 7 || h[499].MarketValue != 499 {
		t.Fatalf("len=%d first=%+v last=%+v", len(h), h[0], h[len(h)-1])
	}
}
