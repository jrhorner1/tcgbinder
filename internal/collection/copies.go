package collection

import (
	"iter"
	"slices"
	"sort"
	"strings"
)

// Purchase is one purchase: copies of a printing in one finish and condition, bought for
// the same price on the same date.
type Purchase struct {
	Quantity     int     `json:"quantity"`
	PricePaid    float64 `json:"price_paid,omitempty"`    // per copy
	AcquiredDate string  `json:"acquired_date,omitempty"` // YYYY-MM-DD
}

// Copies are the copies of one printing owned: finish -> condition -> purchases.
// Each purchase keeps its own price paid and acquired date.
type Copies map[string]map[Condition][]Purchase

// PurchaseRow is one purchase of a card, with its finish and condition.
type PurchaseRow struct {
	Finish    string
	Condition Condition
	Purchase
}

// Conditions in order from best to worst.
var Conditions = []Condition{ConditionNearMint, ConditionLightPlay, ConditionModerate, ConditionHeavyPlay, ConditionDamaged}

func conditionRank(c Condition) int {
	if i := slices.Index(Conditions, c); i >= 0 {
		return i
	}
	return len(Conditions)
}

func normFinish(f string) string { return strings.ToLower(strings.TrimSpace(f)) }

func normCondition(c Condition) Condition {
	if c == "" {
		return ConditionNearMint
	}
	return c
}

func (h Copies) add(finish string, cond Condition, l Purchase) {
	if l.Quantity <= 0 {
		return
	}
	finish, cond = normFinish(finish), normCondition(cond)
	if h[finish] == nil {
		h[finish] = map[Condition][]Purchase{}
	}
	// Copies bought for the same price on the same date are one purchase.
	for i, cur := range h[finish][cond] {
		if cur.PricePaid == l.PricePaid && cur.AcquiredDate == l.AcquiredDate {
			h[finish][cond][i].Quantity += l.Quantity
			return
		}
	}
	h[finish][cond] = append(h[finish][cond], l)
}

// clone deep-copies Copies so cards never share them.
func (h Copies) clone() Copies {
	if h == nil {
		return nil
	}
	out := make(Copies, len(h))
	for f, byCond := range h {
		out[f] = make(map[Condition][]Purchase, len(byCond))
		for c, purchases := range byCond {
			out[f][c] = slices.Clone(purchases)
		}
	}
	return out
}

// AddPurchase adds a purchase to the card. It joins an existing purchase in the same finish
// and condition when the price paid and acquired date match.
func (c *Card) AddPurchase(finish string, cond Condition, l Purchase) {
	c.Copies = c.Copies.clone()
	if c.Copies == nil {
		c.Copies = Copies{}
	}
	c.Copies.add(finish, cond, l)
}

// SetPurchases replaces the purchases in one finish and condition; purchases with no
// copies are dropped, and an empty list removes the condition.
func (c *Card) SetPurchases(finish string, cond Condition, purchases []Purchase) {
	finish, cond = normFinish(finish), normCondition(cond)
	c.Copies = c.Copies.clone()
	if c.Copies == nil {
		c.Copies = Copies{}
	}
	delete(c.Copies[finish], cond)
	for _, l := range purchases {
		c.Copies.add(finish, cond, l)
	}
	if len(c.Copies[finish]) == 0 {
		delete(c.Copies, finish)
	}
	if len(c.Copies) == 0 {
		c.Copies = nil
	}
}

// Purchases lists the purchases in one finish and condition, oldest first.
func (c Card) Purchases(finish string, cond Condition) []Purchase {
	out := slices.Clone(c.Copies[normFinish(finish)][normCondition(cond)])
	sort.SliceStable(out, func(i, j int) bool { return purchaseLess(out[i], out[j]) })
	return out
}

// purchaseLess orders purchases by date (undated last), then price.
func purchaseLess(a, b Purchase) bool {
	if a.AcquiredDate != b.AcquiredDate {
		if a.AcquiredDate == "" || b.AcquiredDate == "" {
			return b.AcquiredDate == ""
		}
		return a.AcquiredDate < b.AcquiredDate
	}
	return a.PricePaid < b.PricePaid
}

// ConditionCopyCount is the number of copies owned in one finish and condition.
func (c Card) ConditionCopyCount(finish string, cond Condition) int {
	n := 0
	for _, l := range c.Copies[normFinish(finish)][normCondition(cond)] {
		n += l.Quantity
	}
	return n
}

// eachPurchase yields every purchase with copies, in no particular order. Totals use it rather
// than PurchaseRows, which allocates and sorts.
func (c Card) eachPurchase() iter.Seq[PurchaseRow] {
	return func(yield func(PurchaseRow) bool) {
		for f, byCond := range c.Copies {
			for cond, purchases := range byCond {
				for _, l := range purchases {
					if l.Quantity > 0 && !yield(PurchaseRow{Finish: f, Condition: cond, Purchase: l}) {
						return
					}
				}
			}
		}
	}
}

// PurchaseRows lists the card's copies, finishes in finishOrder (others after, by name)
// and conditions best first.
func (c Card) PurchaseRows(finishOrder ...string) []PurchaseRow {
	var out []PurchaseRow
	for f, byCond := range c.Copies {
		for cond, purchases := range byCond {
			for _, l := range purchases {
				if l.Quantity > 0 {
					out = append(out, PurchaseRow{Finish: f, Condition: cond, Purchase: l})
				}
			}
		}
	}
	rank := func(f string) int {
		if i := slices.Index(finishOrder, f); i >= 0 {
			return i
		}
		return len(finishOrder)
	}
	sort.Slice(out, func(i, j int) bool {
		a, b := out[i], out[j]
		if ra, rb := rank(a.Finish), rank(b.Finish); ra != rb {
			return ra < rb
		}
		if a.Finish != b.Finish {
			return a.Finish < b.Finish
		}
		if a.Condition != b.Condition {
			return conditionRank(a.Condition) < conditionRank(b.Condition)
		}
		return purchaseLess(a.Purchase, b.Purchase)
	})
	return out
}

// CopyCount is the total number of copies owned.
func (c Card) CopyCount() int {
	n := 0
	for h := range c.eachPurchase() {
		n += h.Quantity
	}
	return n
}

// FinishCopyCount is the number of copies owned in one finish.
func (c Card) FinishCopyCount(finish string) int {
	n := 0
	for _, purchases := range c.Copies[normFinish(finish)] {
		for _, l := range purchases {
			n += l.Quantity
		}
	}
	return n
}

// OwnedFinishes lists the finishes with copies, in finishOrder.
func (c Card) OwnedFinishes(finishOrder ...string) []string {
	var out []string
	for _, h := range c.PurchaseRows(finishOrder...) {
		if !slices.Contains(out, h.Finish) {
			out = append(out, h.Finish)
		}
	}
	return out
}

// PricePaid is the total price paid for copies with a known price; unknown counts copies
// without one.
func (c Card) PricePaid() (total float64, unknown int) {
	for l := range c.eachPurchase() {
		if l.PricePaid > 0 {
			total += l.PricePaid * float64(l.Quantity)
		} else {
			unknown += l.Quantity
		}
	}
	return total, unknown
}

// MarketValue is the market value of every copy (each at its finish's price); unpriced
// counts copies whose finish has no price.
func (c Card) MarketValue() (total float64, unpriced int) {
	for l := range c.eachPurchase() {
		if p, ok := c.Market[l.Finish]; ok {
			total += p * float64(l.Quantity)
		} else {
			unpriced += l.Quantity
		}
	}
	return total, unpriced
}

// BestPrice is the highest market price among the finishes owned: one copy's value.
func (c Card) BestPrice() (float64, bool) {
	best, found := 0.0, false
	for f := range c.Copies {
		if p, ok := c.Market[f]; ok && (!found || p > best) {
			best, found = p, true
		}
	}
	return best, found
}

// EarliestAcquired is the earliest acquired date among the copies, or "".
func (c Card) EarliestAcquired() string {
	out := ""
	for h := range c.eachPurchase() {
		if h.AcquiredDate != "" && (out == "" || h.AcquiredDate < out) {
			out = h.AcquiredDate
		}
	}
	return out
}

// HasFinish reports whether any copies are owned in finish (case-insensitive).
func (c Card) HasFinish(finish string) bool { return c.FinishCopyCount(finish) > 0 }

// HasCondition reports whether any copies are in cond.
func (c Card) HasCondition(cond Condition) bool {
	for f := range c.Copies {
		if c.ConditionCopyCount(f, cond) > 0 {
			return true
		}
	}
	return false
}
