package collection

import (
	"sort"
	"strings"
	"time"
)

// MarketPrice is the current per-copy USD market price for a card's finish, from the
// prices stored at the last refresh. ok is false when the catalog had no price.
func MarketPrice(c Card) (price float64, ok bool) {
	p, ok := c.Market[c.Finish]
	return p, ok
}

// SetStats summarizes the owned cards of one set.
type SetStats struct {
	Code          string
	Name          string
	Released      string // YYYY-MM-DD
	Entries       int
	Copies        int
	UniqueNumbers int // distinct collector numbers owned, any finish/condition
	SetSize       int // cards in the set per Scryfall; 0 if unknown
	Cost          float64
	Market        float64
}

// Completion is the owned fraction of the set (0..1), or -1 when the set size is unknown.
func (s SetStats) Completion() float64 {
	if s.SetSize <= 0 {
		return -1
	}
	return min(float64(s.UniqueNumbers)/float64(s.SetSize), 1)
}

// CardValue is an entry with its total current value.
type CardValue struct {
	Card  Card
	Each  float64
	Total float64
}

// TimePoint is one value on a time series.
type TimePoint struct {
	Date  time.Time
	Value float64
}

// CollectionStats is everything the Home dashboard shows.
type Stats struct {
	Copies, Entries, Sets int
	Cost                  float64 // total price paid, for copies with a known price
	CostUnknown           int     // copies with no purchase price recorded
	Market                float64 // total current market value
	MarketUnknown         int     // copies with no market price
	PricesAsOf            string  // oldest price refresh date across entries
	BySet                 []SetStats
	TopCards              []CardValue
	CostHistory           []TimePoint // cumulative price paid by acquisition date, through today
	// ValueHistory is the value of the cards owned on each date, priced at today's
	// market prices (by acquisition date, through today). Scryfall has no historical
	// prices, so this shows how the collection grew, not how prices moved.
	ValueHistory []TimePoint
}

// today is the end date of the history series; tests replace it.
var today = func() time.Time {
	y, m, d := time.Now().Date()
	return time.Date(y, m, d, 0, 0, 0, 0, time.UTC)
}

// ComputeStats aggregates the collection. sets may be nil when set sizes aren't loaded.
func ComputeStats(cards []Card, sets map[string]SetInfo) Stats {
	var st Stats
	bySet := map[string]*SetStats{}
	numbers := map[string]map[string]bool{}
	costByDate, valueByDate := map[string]float64{}, map[string]float64{}
	var costUndated, valueUndated float64 // cards with no acquired date count from the start

	for _, c := range cards {
		st.Copies += c.Quantity
		st.Entries++

		code := strings.ToUpper(c.Set)
		s := bySet[code]
		if s == nil {
			s = &SetStats{Code: code, Name: c.SetName, Released: c.ReleasedAt}
			if info, ok := sets[code]; ok {
				s.SetSize, s.Released = info.CardCount, info.ReleasedAt
				if info.Name != "" {
					s.Name = info.Name
				}
			}
			bySet[code] = s
			numbers[code] = map[string]bool{}
		}
		s.Entries++
		s.Copies += c.Quantity
		numbers[code][c.Number] = true
		if _, known := sets[code]; !known && c.ReleasedAt != "" && (s.Released == "" || c.ReleasedAt < s.Released) {
			s.Released = c.ReleasedAt // no set metadata: use the earliest card release
		}

		if c.PurchasePrice > 0 {
			cost := c.PurchasePrice * float64(c.Quantity)
			st.Cost += cost
			s.Cost += cost
			if validDate(c.AcquiredDate) {
				costByDate[c.AcquiredDate] += cost
			} else {
				costUndated += cost
			}
		} else {
			st.CostUnknown += c.Quantity
		}

		if p, ok := MarketPrice(c); ok {
			total := p * float64(c.Quantity)
			st.Market += total
			s.Market += total
			st.TopCards = append(st.TopCards, CardValue{Card: c, Each: p, Total: total})
			if validDate(c.AcquiredDate) {
				valueByDate[c.AcquiredDate] += total
			} else {
				valueUndated += total
			}
		} else {
			st.MarketUnknown += c.Quantity
		}
		if c.PricesUpdated != "" && (st.PricesAsOf == "" || c.PricesUpdated < st.PricesAsOf) {
			st.PricesAsOf = c.PricesUpdated
		}
	}

	for code, s := range bySet {
		s.UniqueNumbers = len(numbers[code])
		st.BySet = append(st.BySet, *s)
	}
	st.Sets = len(st.BySet)
	sort.Slice(st.BySet, func(i, j int) bool { // newest set first
		if st.BySet[i].Released != st.BySet[j].Released {
			return st.BySet[i].Released > st.BySet[j].Released
		}
		return st.BySet[i].Code < st.BySet[j].Code
	})

	sort.Slice(st.TopCards, func(i, j int) bool { return st.TopCards[i].Total > st.TopCards[j].Total })
	if len(st.TopCards) > 10 {
		st.TopCards = st.TopCards[:10]
	}

	st.CostHistory = cumulative(costByDate, costUndated)
	st.ValueHistory = cumulative(valueByDate, valueUndated)
	return st
}

func validDate(d string) bool {
	_, err := time.Parse("2006-01-02", d)
	return err == nil
}

// cumulative turns per-date amounts into a running total by date. Undated amounts are
// included from the first point, and the series is extended to today so it ends at
// the collection's current total.
func cumulative(byDate map[string]float64, undated float64) []TimePoint {
	if len(byDate) == 0 && undated == 0 {
		return nil
	}
	dates := make([]string, 0, len(byDate))
	for d := range byDate {
		dates = append(dates, d)
	}
	sort.Strings(dates)
	end := today()
	var out []TimePoint
	running := undated
	for _, d := range dates {
		t, _ := time.Parse("2006-01-02", d)
		running += byDate[d]
		out = append(out, TimePoint{t, running})
	}
	if len(out) == 0 || out[len(out)-1].Date.Before(end) {
		out = append(out, TimePoint{end, running})
	}
	return out
}

// ValueSnapshot records the collection's totals on one day, for the value-over-time chart.
type ValueSnapshot struct {
	Date   string  `json:"date"` // YYYY-MM-DD
	Market float64 `json:"market"`
	Cost   float64 `json:"cost"`
	Copies int     `json:"copies"`
}

// upsertSnapshot replaces the snapshot for snap.Date or appends it, keeping date order.
func upsertSnapshot(history []ValueSnapshot, snap ValueSnapshot) []ValueSnapshot {
	for i := range history {
		if history[i].Date == snap.Date {
			history[i] = snap
			return history
		}
	}
	history = append(history, snap)
	sort.Slice(history, func(i, j int) bool { return history[i].Date < history[j].Date })
	return history
}
