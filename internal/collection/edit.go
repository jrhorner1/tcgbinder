package collection

import (
	"maps"
	"slices"
	"sort"
)

// UniqueCards counts distinct printings (catalog IDs): several copies, finishes
// or conditions of the same printing count once.
func UniqueCards(cards []Card) int {
	seen := map[string]bool{}
	for _, c := range cards {
		seen[c.ID] = true
	}
	return len(seen)
}

// WithPrinting moves an owned card to another printing of the same card: catalog
// data comes from printing and every purchase is kept. Copies in a finish the new
// printing doesn't come in move to its first finish.
func WithPrinting(owned, printing Card) Card {
	c := printing
	c.Attrs = maps.Clone(printing.Attrs)
	c.Market = maps.Clone(printing.Market)
	c.Copies = nil
	for _, h := range owned.PurchaseRows() {
		finish := h.Finish
		if len(c.Finishes) > 0 && !slices.Contains(c.Finishes, finish) {
			finish = c.Finishes[0]
		}
		c.AddPurchase(finish, h.Condition, h.Purchase)
	}
	if c.Language == "" {
		c.Language = owned.Language
	}
	return c
}

// TopByEach ranks priced cards by the value of one copy (their most valuable owned
// finish), most valuable first.
func TopByEach(cards []Card, n int) []CardValue {
	var out []CardValue
	for _, c := range cards {
		if p, ok := c.BestPrice(); ok {
			total, _ := c.MarketValue()
			out = append(out, CardValue{Card: c, Each: p, Total: total})
		}
	}
	sort.SliceStable(out, func(i, j int) bool {
		if out[i].Each != out[j].Each {
			return out[i].Each > out[j].Each
		}
		return DefaultOrder(out[i].Card, out[j].Card) < 0
	})
	if len(out) > n {
		out = out[:n]
	}
	return out
}

// Missing returns the cards of a set that aren't owned (by collector number).
func Missing(setCards, owned []Card) []Card {
	have := map[string]bool{}
	for _, c := range owned {
		have[c.CollectorNumber] = true
	}
	var out []Card
	for _, c := range setCards {
		if !have[c.CollectorNumber] {
			out = append(out, c)
		}
	}
	return out
}

// BulkChange describes an edit applied to many cards at once. Nil fields are left
// unchanged; each applies to every copy of the card.
type BulkChange struct {
	Condition    *Condition // move every copy to this condition
	Finish       *string    // move every copy to this finish, if the printing comes in it
	PricePaid    *float64
	AcquiredDate *string
	Delete       bool
}

// BulkResult reports what a bulk edit did.
type BulkResult struct {
	Updated, Deleted int
	Skipped          []string // cards the change didn't apply to, with why
}

// Apply returns c with the change made (except Delete). ok is false, with a reason,
// when the finish change can't apply because the printing doesn't come in it.
func (change BulkChange) Apply(c Card) (Card, string) {
	skipped := ""
	finish := ""
	if change.Finish != nil {
		finish = normFinish(*change.Finish)
		if len(c.Finishes) > 0 && !slices.Contains(c.Finishes, finish) {
			skipped = c.Name + " (" + c.Set + " · " + c.CollectorNumber + ") isn't printed in " + finish
			finish = ""
		}
	}
	out := c
	out.Copies = nil
	for _, h := range c.PurchaseRows() {
		if finish != "" {
			h.Finish = finish
		}
		if change.Condition != nil {
			h.Condition = *change.Condition
		}
		if change.PricePaid != nil {
			h.PricePaid = *change.PricePaid
		}
		if change.AcquiredDate != nil {
			h.AcquiredDate = *change.AcquiredDate
		}
		out.AddPurchase(h.Finish, h.Condition, h.Purchase) // groups that end up the same merge
	}
	return out, skipped
}

// BulkEdit applies change to the cards with the given keys and saves.
func (cs *Store) BulkEdit(keys []string, change BulkChange) (BulkResult, error) {
	var res BulkResult
	cs.mu.Lock()
	for _, key := range keys {
		c, ok := cs.Cards[key]
		if !ok {
			continue
		}
		if change.Delete {
			delete(cs.Cards, key)
			res.Deleted++
			continue
		}
		edited, skipped := change.Apply(c)
		if skipped != "" {
			res.Skipped = append(res.Skipped, skipped)
		}
		cs.Cards[key] = edited
		res.Updated++
	}
	cs.snapshotLocked()
	cs.mu.Unlock()
	return res, cs.Save()
}
