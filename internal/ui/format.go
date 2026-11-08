package ui

import (
	"strconv"
	"strings"
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
