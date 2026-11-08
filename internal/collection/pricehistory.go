package collection

import (
	"bufio"
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"math"
	"os"
	"path/filepath"
	"sync"
	"time"
)

// PriceHistory holds daily market prices per printing and finish, so the collection's
// real value can be charted over time. It is filled by a one-time MTGJSON download (Load price history) (up
// to 90 days back) and by recording Scryfall prices each day the app is used.
type PriceHistory struct {
	mu           sync.RWMutex
	saveMu       sync.Mutex // serializes Save so writes land in order
	path         string
	loadedPoints int // prices on disk; Save never writes fewer

	HistoryLoaded string `json:"history_loaded,omitempty"` // date price history was last loaded from MTGJSON
	// Cards maps Scryfall ID -> finish -> that printing's daily prices.
	Cards map[string]map[string]*DailyPrices `json:"cards"`
}

// DailyPrices is one printing's prices in one finish, one per day: Cents[i] is the
// USD price in cents on From + i days, or -1 for a day without a price. Prices are
// recorded for nearly every day, so an array is far smaller and faster than a map by
// date, and whole cents (which prices always are) encode much faster than decimals.
type DailyPrices struct {
	From  string  `json:"from"` // YYYY-MM-DD of Cents[0]
	Cents []int32 `json:"cents"`
	start int     // From as a day number (see dayNumber)
}

// toCents and dollars convert between USD and the cents stored.
func toCents(usd float64) int32 { return int32(math.Round(usd * 100)) }
func dollars(c int32) float64   { return float64(c) / 100 }

// noPrice marks a day without a recorded price.
const noPrice = -1

// dayNumber converts YYYY-MM-DD to days since 1970-01-01; ok is false for anything
// else. It avoids time.Parse, which is slow in the value chart's inner loops.
func dayNumber(s string) (n int, ok bool) {
	if len(s) != 10 || s[4] != '-' || s[7] != '-' {
		return 0, false
	}
	num := func(t string) int {
		v := 0
		for _, c := range []byte(t) {
			if c < '0' || c > '9' {
				return -1
			}
			v = v*10 + int(c-'0')
		}
		return v
	}
	y, m, d := num(s[:4]), num(s[5:7]), num(s[8:])
	if y < 0 || m < 1 || m > 12 || d < 1 || d > 31 {
		return 0, false
	}
	return int(time.Date(y, time.Month(m), d, 0, 0, 0, 0, time.UTC).Unix() / 86400), true
}

// dayTime is the UTC midnight of a day number.
func dayTime(n int) time.Time { return time.Unix(int64(n)*86400, 0).UTC() }

// dayOf is the day number of t's calendar date.
func dayOf(t time.Time) int {
	y, m, d := t.Date()
	return int(time.Date(y, m, d, 0, 0, 0, 0, time.UTC).Unix() / 86400)
}

// grow makes the series cover days lo..hi, filling new days with noPrice.
func (dp *DailyPrices) grow(lo, hi int) {
	if len(dp.Cents) == 0 {
		dp.start, dp.Cents = lo, make([]int32, 0, hi-lo+1)
	}
	if lo < dp.start {
		front := make([]int32, dp.start-lo, dp.start-lo+len(dp.Cents))
		for i := range front {
			front[i] = noPrice
		}
		dp.Cents, dp.start = append(front, dp.Cents...), lo
	}
	for end := dp.start + len(dp.Cents) - 1; end < hi; end++ {
		dp.Cents = append(dp.Cents, noPrice)
	}
	dp.From = dayTime(dp.start).Format("2006-01-02")
}

// at returns the price on day d, if one was recorded.
func (dp *DailyPrices) at(d int) (float64, bool) {
	if i := d - dp.start; i >= 0 && i < len(dp.Cents) && dp.Cents[i] >= 0 {
		return dollars(dp.Cents[i]), true
	}
	return 0, false
}

// first and last are the indexes of the first and last recorded prices (-1 if none).
func (dp *DailyPrices) first() int {
	for i, p := range dp.Cents {
		if p >= 0 {
			return i
		}
	}
	return -1
}

func (dp *DailyPrices) last() int {
	for i := len(dp.Cents) - 1; i >= 0; i-- {
		if dp.Cents[i] >= 0 {
			return i
		}
	}
	return -1
}

// Price returns the price recorded for a printing and finish on date (YYYY-MM-DD).
func (h *PriceHistory) Price(id, finish, date string) (float64, bool) {
	h.mu.RLock()
	defer h.mu.RUnlock()
	d, ok := dayNumber(date)
	if dp := h.Cards[id][finish]; ok && dp != nil {
		return dp.at(d)
	}
	return 0, false
}

// Days is the number of days with a price for a printing and finish.
func (h *PriceHistory) Days(id, finish string) int {
	h.mu.RLock()
	defer h.mu.RUnlock()
	n := 0
	if dp := h.Cards[id][finish]; dp != nil {
		for _, p := range dp.Cents {
			if p >= 0 {
				n++
			}
		}
	}
	return n
}

// OpenPriceHistory reads the history file at path; a missing file gives an empty
// history. A file that can't be parsed is never overwritten: it is renamed to
// "<path>.unreadable-<timestamp>" so it can be recovered, and a fresh history is
// returned together with an error describing what happened.
func OpenPriceHistory(path string) (*PriceHistory, error) {
	fresh := func() *PriceHistory {
		return &PriceHistory{path: path, Cards: map[string]map[string]*DailyPrices{}}
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
		h.Cards = map[string]map[string]*DailyPrices{}
	}
	for id, byFinish := range h.Cards {
		for finish, dp := range byFinish {
			start, ok := dayNumber(dp.From)
			if !ok && len(dp.Cents) > 0 {
				return fresh(), fmt.Errorf("price history %s: %s %s starts on %q, not a date", path, id, finish, dp.From)
			}
			dp.start = start
		}
	}
	h.loadedPoints = h.pointsLocked()
	return h, nil
}

// pointsLocked counts recorded prices. Callers hold h.mu.
func (h *PriceHistory) pointsLocked() int {
	n := 0
	for _, byFinish := range h.Cards {
		for _, dp := range byFinish {
			for _, p := range dp.Cents {
				if p >= 0 {
					n++
				}
			}
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

	tmp, err := os.CreateTemp(filepath.Dir(h.path), "price-history_*.tmp")
	if err != nil {
		return err
	}
	defer os.Remove(tmp.Name())
	// Stream the JSON to the file (buffered) rather than building it in memory.
	buf := bufio.NewWriterSize(tmp, 1<<16)
	h.mu.RLock()
	points := h.pointsLocked()
	err = json.NewEncoder(buf).Encode(h)
	h.mu.RUnlock()
	if err == nil && points < h.loadedPoints {
		err = fmt.Errorf("refusing to save price history with %d prices over %d loaded: history is never truncated", points, h.loadedPoints)
	}
	if err == nil {
		err = buf.Flush()
	}
	if err == nil {
		err = tmp.Sync()
	}
	if cerr := tmp.Close(); err == nil {
		err = cerr
	}
	if err != nil {
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
	d, ok := dayNumber(date)
	if !ok || price < 0 {
		return
	}
	dp := h.series(id, finish)
	dp.grow(d, d)
	dp.Cents[d-dp.start] = toCents(price)
}

// series returns (creating if needed) the prices for a printing and finish.
func (h *PriceHistory) series(id, finish string) *DailyPrices {
	byFinish := h.Cards[id]
	if byFinish == nil {
		byFinish = map[string]*DailyPrices{}
		h.Cards[id] = byFinish
	}
	dp := byFinish[finish]
	if dp == nil {
		dp = &DailyPrices{}
		byFinish[finish] = dp
	}
	return dp
}

// Merge adds a historyLoaded series for one printing. Dates already recorded keep their
// value (prices the app recorded itself are not replaced), and nothing is removed, so
// days older than MTGJSON's 90-day window stay in the history.
func (h *PriceHistory) Merge(id string, byFinish map[string]map[string]float64) {
	h.mu.Lock()
	defer h.mu.Unlock()
	for finish, byDate := range byFinish {
		lo, hi, any := 0, 0, false
		for date, p := range byDate {
			if d, ok := dayNumber(date); ok && p >= 0 {
				if !any || d < lo {
					lo = d
				}
				if !any || d > hi {
					hi = d
				}
				any = true
			}
		}
		if !any {
			continue
		}
		dp := h.series(id, finish)
		dp.grow(lo, hi) // once, rather than per date
		for date, p := range byDate {
			if d, ok := dayNumber(date); ok && p >= 0 && dp.Cents[d-dp.start] < 0 {
				dp.Cents[d-dp.start] = toCents(p)
			}
		}
	}
}

// MarkHistoryLoaded records that price history was loaded on date.
func (h *PriceHistory) MarkHistoryLoaded(date string) {
	h.mu.Lock()
	h.HistoryLoaded = date
	h.mu.Unlock()
}

// HistoryLoadedOn returns the date of the last price history load, or "".
func (h *PriceHistory) HistoryLoadedOn() string {
	h.mu.RLock()
	defer h.mu.RUnlock()
	return h.HistoryLoaded
}

// RecordCurrent stores each card's current market prices (every finish) for date.
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
	earliest, found := 0, false
	for _, byFinish := range h.Cards {
		for _, dp := range byFinish {
			if i := dp.first(); i >= 0 && (!found || dp.start+i < earliest) {
				earliest, found = dp.start+i, true
			}
		}
	}
	if !found {
		return time.Time{}
	}
	return dayTime(earliest)
}

// MarketSeries returns the collection's market value for each day from `from` through
// `to` on which any price is recorded, plus the range's end points: the copies owned that day (by acquired date;
// undated copies always count) times that day's price for their finish. A card without
// a price that day uses its nearest earlier recorded price, then its nearest later one,
// then its current Scryfall price, so gaps in the data don't read as value drops.
func (h *PriceHistory) MarketSeries(cards []Card, from, to time.Time) []TimePoint {
	h.mu.RLock()
	defer h.mu.RUnlock()

	fromD, toD := dayOf(from), dayOf(to)
	if toD < fromD {
		return []TimePoint{}
	}
	// One item per purchase, each with its own acquired date.
	type priced struct {
		qty         float64
		acquired    int // first day owned; undated purchases count from the start
		prices      *DailyPrices
		first, last int // indexes of the first and last recorded prices
		now         float64
		has         bool
	}
	var items []priced
	marked := make([]bool, toD-fromD+1) // days in range with any recorded price
	earlier := false                    // some price is from before the range
	for _, c := range cards {
		for l := range c.eachPurchase() {
			p := priced{qty: float64(l.Quantity), acquired: -1 << 31, first: -1}
			if d, ok := dayNumber(l.AcquiredDate); ok {
				p.acquired = d
			}
			p.now, p.has = MarketPrice(c, l.Finish)
			if dp := h.Cards[c.ID][l.Finish]; dp != nil {
				if p.first = dp.first(); p.first >= 0 {
					p.prices, p.last = dp, dp.last()
					lo, hi := max(fromD, dp.start+p.first), min(toD, dp.start+p.last)
					for d := lo; d <= hi; d++ {
						if dp.Cents[d-dp.start] >= 0 {
							marked[d-fromD] = true
						}
					}
					earlier = earlier || dp.start+p.first < fromD
				}
			}
			items = append(items, p)
		}
	}
	// Always span the requested range: the last day (prices carried forward) and the
	// first day when there is history from before it.
	if len(items) > 0 {
		marked[toD-fromD] = true
		marked[0] = marked[0] || earlier
	}
	var days []int
	for i, m := range marked {
		if m {
			days = append(days, fromD+i)
		}
	}

	totals := make([]float64, len(days))
	for _, p := range items {
		for k, d := range days {
			if d < p.acquired {
				continue // not owned yet
			}
			switch {
			case p.prices != nil:
				// That day's price, else the nearest earlier one, else the first.
				i := min(d-p.prices.start, p.last)
				for i > p.first && p.prices.Cents[i] < 0 {
					i--
				}
				totals[k] += dollars(p.prices.Cents[max(i, p.first)]) * p.qty
			case p.has:
				totals[k] += p.now * p.qty
			}
		}
	}
	out := make([]TimePoint, len(days))
	for k, d := range days {
		out[k] = TimePoint{dayTime(d), totals[k]}
	}
	return out
}
