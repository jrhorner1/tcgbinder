package collection

import (
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"sort"
	"sync"
	"time"
)

// PriceHistory holds daily market prices per printing and finish, so the collection's
// real value can be charted over time. It is filled by a one-time MTGJSON backfill (up
// to 90 days back) and by recording Scryfall prices each day the app is used.
type PriceHistory struct {
	mu           sync.RWMutex
	saveMu       sync.Mutex // serializes Save so writes land in order
	path         string
	loadedPoints int // prices on disk; Save never writes fewer

	Backfilled string `json:"backfilled,omitempty"` // date of the last MTGJSON backfill
	// Cards maps Scryfall ID -> finish -> date (YYYY-MM-DD) -> USD price.
	Cards map[string]map[string]map[string]float64 `json:"cards"`
}

// LoadPriceHistory reads the history file at path; a missing file gives an empty
// history. A file that can't be parsed is never overwritten: it is renamed to
// "<path>.unreadable-<timestamp>" so it can be recovered, and a fresh history is
// returned together with an error describing what happened.
func LoadPriceHistory(path string) (*PriceHistory, error) {
	fresh := func() *PriceHistory {
		return &PriceHistory{path: path, Cards: map[string]map[string]map[string]float64{}}
	}
	b, err := os.ReadFile(path)
	if errors.Is(err, fs.ErrNotExist) {
		return fresh(), nil
	}
	if err != nil {
		return nil, err
	}
	h := fresh()
	if err := json.Unmarshal(b, h); err != nil {
		backup := fmt.Sprintf("%s.unreadable-%s", path, time.Now().Format("20060102-150405"))
		if rerr := os.Rename(path, backup); rerr != nil {
			return nil, fmt.Errorf("price history %s is unreadable (%v) and could not be set aside: %w", path, err, rerr)
		}
		return fresh(), fmt.Errorf("price history %s was unreadable (%v); kept it as %s and started a new one", path, err, backup)
	}
	if h.Cards == nil {
		h.Cards = map[string]map[string]map[string]float64{}
	}
	h.loadedPoints = h.pointsLocked()
	return h, nil
}

// pointsLocked counts recorded prices. Callers hold h.mu.
func (h *PriceHistory) pointsLocked() int {
	n := 0
	for _, byFinish := range h.Cards {
		for _, byDate := range byFinish {
			n += len(byDate)
		}
	}
	return n
}

// Points is the number of recorded daily prices.
func (h *PriceHistory) Points() int {
	h.mu.RLock()
	defer h.mu.RUnlock()
	return h.pointsLocked()
}

// Save writes the history atomically: a temp file is written, synced to disk, then
// renamed over the old file. Saves are serialized so a slower save can never replace
// a newer one, and a save that would hold fewer prices than were loaded is refused —
// history is only ever added to.
func (h *PriceHistory) Save() error {
	h.saveMu.Lock()
	defer h.saveMu.Unlock()

	h.mu.RLock()
	points := h.pointsLocked()
	b, err := json.Marshal(h)
	h.mu.RUnlock()
	if err != nil {
		return err
	}
	if points < h.loadedPoints {
		return fmt.Errorf("refusing to save price history with %d prices over %d loaded: history is never truncated", points, h.loadedPoints)
	}
	tmp, err := os.CreateTemp(filepath.Dir(h.path), "price-history_*.tmp")
	if err != nil {
		return err
	}
	defer os.Remove(tmp.Name())
	if _, err := tmp.Write(b); err != nil {
		tmp.Close()
		return err
	}
	if err := tmp.Sync(); err != nil {
		tmp.Close()
		return err
	}
	if err := tmp.Close(); err != nil {
		return err
	}
	if err := os.Rename(tmp.Name(), h.path); err != nil {
		return err
	}
	h.loadedPoints = points
	return nil
}

// Set records one price.
func (h *PriceHistory) Set(id, finish, date string, price float64) {
	h.mu.Lock()
	defer h.mu.Unlock()
	h.setLocked(id, finish, date, price)
}

func (h *PriceHistory) setLocked(id, finish, date string, price float64) {
	byFinish := h.Cards[id]
	if byFinish == nil {
		byFinish = map[string]map[string]float64{}
		h.Cards[id] = byFinish
	}
	byDate := byFinish[finish]
	if byDate == nil {
		byDate = map[string]float64{}
		byFinish[finish] = byDate
	}
	byDate[date] = price
}

// Merge adds a backfilled series for one printing. Dates already recorded keep their
// value (prices the app recorded itself are not replaced), and nothing is removed, so
// days older than MTGJSON's 90-day window stay in the history.
func (h *PriceHistory) Merge(id string, byFinish map[string]map[string]float64) {
	h.mu.Lock()
	defer h.mu.Unlock()
	for finish, byDate := range byFinish {
		for date, p := range byDate {
			if _, have := h.Cards[id][finish][date]; !have {
				h.setLocked(id, finish, date, p)
			}
		}
	}
}

// MarkBackfilled records that a backfill finished on date.
func (h *PriceHistory) MarkBackfilled(date string) {
	h.mu.Lock()
	h.Backfilled = date
	h.mu.Unlock()
}

// BackfilledOn returns the date of the last backfill, or "".
func (h *PriceHistory) BackfilledOn() string {
	h.mu.RLock()
	defer h.mu.RUnlock()
	return h.Backfilled
}

// RecordCurrent stores each entry's current market prices (every finish) for date.
func (h *PriceHistory) RecordCurrent(cards []Card, date string) {
	h.mu.Lock()
	defer h.mu.Unlock()
	for _, c := range cards {
		if c.ID == "" {
			continue
		}
		for finish, p := range c.Market {
			h.setLocked(c.ID, finish, date, p)
		}
	}
}

// Start returns the earliest date with any recorded price, or zero time.
func (h *PriceHistory) Start() time.Time {
	h.mu.RLock()
	defer h.mu.RUnlock()
	earliest := ""
	for _, byFinish := range h.Cards {
		for _, byDate := range byFinish {
			for d := range byDate {
				if earliest == "" || d < earliest {
					earliest = d
				}
			}
		}
	}
	t, _ := time.Parse("2006-01-02", earliest)
	return t
}

// MarketSeries returns the collection's market value for each day from `from` through
// `to` on which any price is recorded, plus the range's end points: the copies owned that day (by acquired date;
// undated copies always count) times that day's price for their finish. A card without
// a price that day uses its nearest earlier recorded price, then its nearest later one,
// then its current Scryfall price, so gaps in the data don't read as value drops.
func (h *PriceHistory) MarketSeries(cards []Card, from, to time.Time) []TimePoint {
	h.mu.RLock()
	defer h.mu.RUnlock()

	fromS, toS := from.Format("2006-01-02"), to.Format("2006-01-02")
	daySet := map[string]bool{}
	type priced struct {
		card  Card
		dates []string // sorted dates with a price for this card's finish
		price map[string]float64
		now   float64
		has   bool
	}
	items := make([]priced, 0, len(cards))
	for _, c := range cards {
		p := priced{card: c}
		p.now, p.has = MarketPrice(c)
		if byDate := h.Cards[c.ID][c.Finish]; len(byDate) > 0 {
			p.price = byDate
			for d := range byDate {
				p.dates = append(p.dates, d)
				if d >= fromS && d <= toS {
					daySet[d] = true
				}
			}
			sort.Strings(p.dates)
		}
		items = append(items, p)
	}
	// Always span the requested range: the last day (prices carried forward) and the
	// first day when there is history from before it.
	if len(items) > 0 {
		daySet[toS] = true
		for _, p := range items {
			if len(p.dates) > 0 && p.dates[0] < fromS {
				daySet[fromS] = true
				break
			}
		}
	}
	days := make([]string, 0, len(daySet))
	for d := range daySet {
		days = append(days, d)
	}
	sort.Strings(days)

	out := make([]TimePoint, 0, len(days))
	for _, d := range days {
		total := 0.0
		for _, p := range items {
			if validDate(p.card.AcquiredDate) && p.card.AcquiredDate > d {
				continue // not owned yet
			}
			price, ok := 0.0, false
			if len(p.dates) > 0 {
				i := sort.SearchStrings(p.dates, d) // first date >= d
				switch {
				case i < len(p.dates) && p.dates[i] == d:
					price, ok = p.price[d], true
				case i > 0:
					price, ok = p.price[p.dates[i-1]], true
				default:
					price, ok = p.price[p.dates[0]], true
				}
			} else if p.has {
				price, ok = p.now, true
			}
			if ok {
				total += price * float64(p.card.Quantity)
			}
		}
		t, _ := time.Parse("2006-01-02", d)
		out = append(out, TimePoint{t, total})
	}
	return out
}
