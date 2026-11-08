package service

import (
	"time"

	"github.com/jrhorner1/mtgmanager/internal/collection"
)

// ChartRanges are the Value Over Time ranges offered by the UIs.
var ChartRanges = []string{"30D", "90D", "1Y", "All"}

// DefaultChartRange matches the 90 days of history MTGJSON provides.
const DefaultChartRange = "90D"

// Today is the current date at midnight UTC, the end of every chart.
func Today() time.Time {
	y, m, d := time.Now().Date()
	return time.Date(y, m, d, 0, 0, 0, 0, time.UTC)
}

// RangeStart is the first day shown for a range; zero means "everything".
func RangeStart(r string, today time.Time) time.Time {
	switch r {
	case "30D":
		return today.AddDate(0, 0, -30)
	case "90D":
		return today.AddDate(0, 0, -90)
	case "1Y":
		return today.AddDate(-1, 0, 0)
	}
	return time.Time{}
}

// ValueChart is the data behind the Value Over Time chart.
type ValueChart struct {
	From, To     time.Time              // From is zero for "All"
	HistoryStart time.Time              // first day with recorded prices; zero if none
	Market       []collection.TimePoint // real daily market value
	Paid         []collection.TimePoint // running price paid (step line)
	// Estimate values cards at today's prices before recorded prices begin (step
	// line); nil when the real line covers the whole range.
	Estimate []collection.TimePoint
}

// BuildValueChart computes the chart for cards over the named range.
func BuildValueChart(cards []collection.Card, st collection.Stats, prices *collection.PriceHistory, rangeName string, today time.Time) ValueChart {
	from := RangeStart(rangeName, today)
	vc := ValueChart{From: from, To: today, HistoryStart: prices.Start()}
	marketFrom := from
	if marketFrom.IsZero() {
		marketFrom = vc.HistoryStart
	}
	vc.Market = prices.MarketSeries(cards, marketFrom, today)
	vc.Paid = ClipStep(st.CostHistory, from)
	if vc.HistoryStart.IsZero() || from.IsZero() || from.Before(vc.HistoryStart) {
		est := ClipStep(st.ValueHistory, from)
		if !vc.HistoryStart.IsZero() {
			est = EndAt(est, vc.HistoryStart) // real prices take over from here
		}
		vc.Estimate = est
	}
	return vc
}

// ClipStep limits a step series to dates on or after from, starting with the value in
// effect on that day so the line doesn't begin at zero mid-range.
func ClipStep(points []collection.TimePoint, from time.Time) []collection.TimePoint {
	if from.IsZero() {
		return points
	}
	var out []collection.TimePoint
	for i, p := range points {
		if p.Date.After(from) {
			if len(out) == 0 && i > 0 {
				out = append(out, collection.TimePoint{Date: from, Value: points[i-1].Value})
			}
			out = append(out, p)
		} else if p.Date.Equal(from) {
			out = append(out, p)
		}
	}
	if len(out) == 0 && len(points) > 0 { // everything happened before the range
		last := points[len(points)-1]
		out = []collection.TimePoint{{Date: from, Value: last.Value}, {Date: last.Date, Value: last.Value}}
		if !last.Date.After(from) {
			out[1].Date = Today()
		}
	}
	return out
}

// EndAt keeps a step series' points before until and ends it with the value in effect
// on that day.
func EndAt(points []collection.TimePoint, until time.Time) []collection.TimePoint {
	var out []collection.TimePoint
	for _, p := range points {
		if !p.Date.Before(until) {
			break
		}
		out = append(out, p)
	}
	if len(out) > 0 {
		out = append(out, collection.TimePoint{Date: until, Value: out[len(out)-1].Value})
	}
	return out
}
