//go:build darwin

package apple

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/jrhorner1/tcgbinder/internal/games/mtg"
)

// TestReadAndIdentifyPhotos reads every photo in MTG_SCAN_DIR with Vision and
// identifies it against the live Scryfall API. A file named "<set>-<number>-*.jpg"
// must come out as that printing. Skipped unless MTG_SCAN_DIR is set.
func TestReadAndIdentifyPhotos(t *testing.T) {
	dir := os.Getenv("MTG_SCAN_DIR")
	if dir == "" {
		t.Skip("set MTG_SCAN_DIR to a folder of card photos")
	}
	files, _ := filepath.Glob(filepath.Join(dir, "*.jpg"))
	g := mtg.New()
	for _, f := range files {
		base := filepath.Base(f)
		parts := strings.SplitN(base, "-", 3)
		data, _ := os.ReadFile(f)
		start := time.Now()
		r, err := ReadImage(data)
		if err != nil {
			t.Errorf("%s: %v", base, err)
			continue
		}
		read := time.Since(start)
		res, err := g.Identify(r)
		if err != nil {
			t.Errorf("%s: identify: %v", base, err)
			continue
		}
		got := "no match"
		if len(res.Candidates) > 0 {
			c := res.Candidates[0]
			got = strings.ToLower(c.Set) + "-" + c.CollectorNumber + " " + c.Name
		}
		if os.Getenv("MTG_SCAN_TEXT") != "" {
			t.Logf("%s text:\n%s", base, r.Text())
		}
		t.Logf("%-22s card=%-5v read %-28q in %4dms → %s (exact=%v, %d candidates) %s", base, r.CardFound, res.Read, read.Milliseconds(), got, res.Exact, len(res.Candidates), res.Note)
		want := parts[0] + "-" + parts[1]
		if !res.Exact && len(parts) == 3 { // a choice: the right printing must be near the top
			for i, c := range res.Candidates {
				if i < 5 && strings.ToLower(c.Set)+"-"+c.CollectorNumber == want {
					got = want + " " + c.Name
				}
			}
		}
		if len(parts) == 3 && !strings.HasPrefix(got, want+" ") {
			t.Errorf("%s: identified as %s; text read:\n%s", base, got, r.Text())
		}
	}
}
