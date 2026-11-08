package ui

import (
	"fmt"
	"strconv"
	"strings"

	"github.com/jrhorner1/tcgbinder/internal/collection"
	"github.com/jrhorner1/tcgbinder/internal/game"
)

// formatUSD renders 1234.5 as "$1,234.50".
func formatUSD(v float64) string {
	neg := v < 0
	if neg {
		v = -v
	}
	s := strconv.FormatFloat(v, 'f', 2, 64)
	intPart, frac := s[:len(s)-3], s[len(s)-3:]
	var b strings.Builder
	for i, r := range intPart {
		if i > 0 && (len(intPart)-i)%3 == 0 {
			b.WriteByte(',')
		}
		b.WriteRune(r)
	}
	out := "$" + b.String() + frac
	if neg {
		return "-" + out
	}
	return out
}

// setNumber shows where a card is printed, e.g. "M19 · 201".
func setNumber(set, number string) string {
	if number == "" {
		return set
	}
	return set + " · " + number
}

// copiesSummary describes a card's copies compactly: "×3" when they're all in the
// default finish and near mint, otherwise per finish, e.g. "Normal ×2 · Foil ×1 (1 LP)".
func copiesSummary(c collection.Card, g game.Game) string {
	finishes := c.OwnedFinishes(g.Finishes()...)
	var parts []string
	simple := len(finishes) == 1 && len(g.Finishes()) > 0 && finishes[0] == g.Finishes()[0]
	for _, f := range finishes {
		var worn []string
		for _, cond := range collection.Conditions[1:] {
			if n := c.ConditionCopyCount(f, cond); n > 0 {
				worn = append(worn, fmt.Sprintf("%d %s", n, cond))
				simple = false
			}
		}
		part := fmt.Sprintf("%s ×%d", g.FinishName(f), c.FinishCopyCount(f))
		if len(worn) > 0 {
			part += " (" + strings.Join(worn, ", ") + ")"
		}
		parts = append(parts, part)
	}
	if simple {
		return fmt.Sprintf("×%d", c.CopyCount())
	}
	return strings.Join(parts, " · ")
}
