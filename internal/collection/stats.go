package collection

import (
	"sort"
	"strings"
	"time"
)

// MarketPrice is the current per-copy USD market price of a card in a finish, from
// the prices stored at the last refresh. ok is false when the catalog had no price.
func MarketPrice(c Card, finish string) (price float64, ok bool) {
	p, ok := c.Market[finish]
	return p, ok
}

// SetStats summarizes the owned cards of one set.
type SetStats struct {
	Code        string
	Name        string
	Released    string // YYYY-MM-DD
	UniqueCards int
	Cards       int
	Collected   int // distinct collector numbers owned, any finish/condition
	SetSize     int // cards in the set per Scryfall; 0 if unknown
	PricePaid   float64
	MarketValue float64
}

// Completion is the owned fraction of the set (0..1), or -1 when the set size is unknown.
func (s SetStats) Completion() float64 {
	if s.SetSize <= 0 {
		return -1
	}
	return min(float64(s.Collected)/float64(s.SetSize), 1)
}

// CardValue is a card with its total current value.
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
	Cards, UniqueCards, Sets int
	PricePaid                float64 // total price paid, for copies with a known price
	PricePaidUnknown         int     // copies with no price paid recorded
	MarketValue              float64 // total current market value
	Unpriced                 int     // copies with no market price
	PricesAsOf               string  // oldest price refresh date across cards
	BySet                    []SetStats
	TopCards                 []CardValue
	PricePaidHistory         []TimePoint // cumulative price paid by acquisition date, through today
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

// topCards is how many of the most valuable cards Stats lists.
const topCards = 10

// keepTop inserts v into top (most valuable first) and keeps at most n, so finding
// the top cards doesn't copy and sort every card. Equal totals keep their order.
func keepTop(top []CardValue, v CardValue, n int) []CardValue {
	i := sort.Search(len(top), func(i int) bool { return top[i].Total < v.Total })
	if i >= n {
		return top
	}
	if len(top) < n {
		top = append(top, CardValue{})
	}
	copy(top[i+1:], top[i:])
	top[i] = v
	return top
}

// ComputeStats aggregates the collection. sets may be nil when set sizes aren't loaded.
func ComputeStats(cards []Card, sets map[string]SetInfo) Stats {
	var st Stats
	bySet := map[string]*SetStats{}
	numbers := map[string]map[string]bool{}
	paidByDate, valueByDate := map[string]float64{}, map[string]float64{}
	var paidUndated, valueUndated float64 // cards with no acquired date count from the start

	for _, c := range cards {
		qty := c.CopyCount()
		st.Cards += qty
		st.UniqueCards++

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
		s.UniqueCards++
		s.Cards += qty
		numbers[code][c.CollectorNumber] = true
		if _, known := sets[code]; !known && c.ReleasedAt != "" && (s.Released == "" || c.ReleasedAt < s.Released) {
			s.Released = c.ReleasedAt // no set metadata: use the earliest card release
		}

		var cardValue float64
		priced := false
		for l := range c.eachPurchase() { // each finish and condition, with its own date
			if l.PricePaid > 0 {
				paid := l.PricePaid * float64(l.Quantity)
				st.PricePaid += paid
				s.PricePaid += paid
				if validDate(l.AcquiredDate) {
					paidByDate[l.AcquiredDate] += paid
				} else {
					paidUndated += paid
				}
			} else {
				st.PricePaidUnknown += l.Quantity
			}
			if p, ok := MarketPrice(c, l.Finish); ok {
				total := p * float64(l.Quantity)
				st.MarketValue += total
				s.MarketValue += total
				cardValue += total
				priced = true
				if validDate(l.AcquiredDate) {
					valueByDate[l.AcquiredDate] += total
				} else {
					valueUndated += total
				}
			} else {
				st.Unpriced += l.Quantity
			}
		}
		if priced {
			each := 0.0
			if qty > 0 {
				each = cardValue / float64(qty)
			}
			st.TopCards = keepTop(st.TopCards, CardValue{Card: c, Each: each, Total: cardValue}, topCards)
		}
		if c.PricesUpdated != "" && (st.PricesAsOf == "" || c.PricesUpdated < st.PricesAsOf) {
			st.PricesAsOf = c.PricesUpdated
		}
	}

	for code, s := range bySet {
		s.Collected = len(numbers[code])
		st.BySet = append(st.BySet, *s)
	}
	st.Sets = len(st.BySet)
	sort.Slice(st.BySet, func(i, j int) bool { // newest set first
		if st.BySet[i].Released != st.BySet[j].Released {
			return st.BySet[i].Released > st.BySet[j].Released
		}
		return st.BySet[i].Code < st.BySet[j].Code
	})

	st.PricePaidHistory = cumulative(paidByDate, paidUndated)
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
	Date        string  `json:"date"` // YYYY-MM-DD
	MarketValue float64 `json:"market_value"`
	PricePaid   float64 `json:"price_paid"`
	Cards       int     `json:"cards"`
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

// Card orders for the collection list (the Card order setting).
const (
	OrderName   = "name"   // by name (DefaultOrder)
	OrderNewest = "newest" // by set, newest release first, then collector number
	OrderOldest = "oldest" // by set, oldest release first, then collector number
	OrderValue  = "value"  // most valuable first (every copy), then by name
)

// SortCards returns the cards in one of the Card orders, leaving the input as it is.
func SortCards(cards []Card, order string) []Card {
	idx := make([]int, len(cards))
	for i := range idx {
		idx[i] = i
	}
	bySet := func(a, b *Card, newestFirst bool) int {
		if c := strings.Compare(a.ReleasedAt, b.ReleasedAt); c != 0 {
			if newestFirst {
				return -c
			}
			return c
		}
		if c := compareFold(a.Set, b.Set); c != 0 {
			return c
		}
		if c := compareCollectorNumber(a.CollectorNumber, b.CollectorNumber); c != 0 {
			return c
		}
		return DefaultOrder(*a, *b)
	}
	return sortedCards(cards, idx, func(a, b *Card) int {
		switch order {
		case OrderNewest:
			return bySet(a, b, true)
		case OrderOldest:
			return bySet(a, b, false)
		case OrderValue:
			av, _ := a.MarketValue()
			bv, _ := b.MarketValue()
			if c := compareFloat(bv, av); c != 0 {
				return c
			}
		}
		return DefaultOrder(*a, *b)
	})
}
