package ui

import (
	"testing"
)

func TestFormatUSD(t *testing.T) {
	for v, want := range map[float64]string{0: "$0.00", 5.5: "$5.50", 1234.567: "$1,234.57", 1234567: "$1,234,567.00", -42.1: "-$42.10"} {
		if got := formatUSD(v); got != want {
			t.Errorf("formatUSD(%v) = %q, want %q", v, got, want)
		}
	}
}
