package service

import (
	"testing"
	"time"

	"github.com/jrhorner1/mtgmanager/internal/collection"
)

func day(s string) time.Time {
	t, _ := time.Parse("2006-01-02", s)
	return t
}

func TestClipStep(t *testing.T) {
	pts := []collection.TimePoint{{Date: day("2018-08-08"), Value: 50}, {Date: day("2026-11-01"), Value: 80}, {Date: day("2026-11-07"), Value: 80}}
	got := ClipStep(pts, day("2026-08-09"))
	if len(got) != 3 || !got[0].Date.Equal(day("2026-08-09")) || got[0].Value != 50 || got[1].Value != 80 {
		t.Fatalf("clip = %+v", got)
	}
	if all := ClipStep(pts, day("2018-01-01")); len(all) != 3 {
		t.Fatalf("a range before all data should keep every point: %+v", all)
	}
	if len(ClipStep(pts, time.Time{})) != 3 {
		t.Fatal("zero start means All")
	}
}

func TestEndAt(t *testing.T) {
	pts := []collection.TimePoint{{Date: day("2018-08-08"), Value: 50}, {Date: day("2026-11-01"), Value: 80}}
	got := EndAt(pts, day("2026-08-09"))
	if len(got) != 2 || got[1].Value != 50 || !got[1].Date.Equal(day("2026-08-09")) {
		t.Fatalf("endAt = %+v", got)
	}
	if len(EndAt(pts, day("2010-01-01"))) != 0 {
		t.Fatal("nothing before the cut-off should give an empty series")
	}
}
