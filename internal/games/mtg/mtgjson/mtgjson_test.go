package mtgjson

import (
	"bytes"
	"compress/gzip"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/ulikunitz/xz"
)

const allPricesFixture = `{
  "meta": {"date": "2026-11-07", "version": "5.3.0"},
  "data": {
    "uuid-shock": {
      "mtgo": {"cardhoarder": {"retail": {"normal": {"2026-11-07": 0.03}}}},
      "paper": {
        "tcgplayer":   {"buylist": {}, "retail": {"normal": {"2026-11-06": 0.20, "2026-11-07": 0.25}, "foil": {"2026-11-07": 1.10}}, "currency": "USD"},
        "cardkingdom": {"retail": {"normal": {"2026-11-07": 0.35}, "etched": {"2026-11-07": 3.00}}, "currency": "USD"}
      }
    },
    "uuid-other": {"paper": {"tcgplayer": {"retail": {"normal": {"2026-11-07": 9.99}}}}},
    "uuid-ckonly": {"paper": {"cardkingdom": {"retail": {"normal": {"2026-11-05": 1.50}}}}}
  }
}`

func TestParseAllPrices(t *testing.T) {
	got, err := ParseAllPrices(strings.NewReader(allPricesFixture), map[string]bool{"uuid-shock": true, "uuid-ckonly": true})
	if err != nil {
		t.Fatal(err)
	}
	if _, ok := got["uuid-other"]; ok {
		t.Error("unwanted card was kept")
	}
	shock := got["uuid-shock"]
	if shock["nonfoil"]["2026-11-06"] != 0.20 || shock["nonfoil"]["2026-11-07"] != 0.25 || shock["foil"]["2026-11-07"] != 1.10 {
		t.Errorf("TCGplayer prices should win: %+v", shock)
	}
	if shock["etched"]["2026-11-07"] != 3.00 {
		t.Errorf("a finish TCGplayer lacks should come from Card Kingdom: %+v", shock)
	}
	if got["uuid-ckonly"]["nonfoil"]["2026-11-05"] != 1.50 {
		t.Errorf("Card Kingdom fallback: %+v", got["uuid-ckonly"])
	}
}

func gz(t *testing.T, s string) []byte {
	var b bytes.Buffer
	w := gzip.NewWriter(&b)
	w.Write([]byte(s))
	w.Close()
	return b.Bytes()
}

func TestUUIDsAndPriceHistoryOverHTTP(t *testing.T) {
	var xzBuf bytes.Buffer
	xw, err := xz.NewWriter(&xzBuf)
	if err != nil {
		t.Fatal(err)
	}
	xw.Write([]byte(allPricesFixture))
	xw.Close()

	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/M19.json.gz":
			w.Write(gz(t, `{"data":{"code":"M19",
				"cards":[{"uuid":"uuid-shock","identifiers":{"scryfallId":"sf-shock"}}],
				"tokens":[{"uuid":"uuid-angel","identifiers":{"scryfallId":"sf-angel"}}]}}`))
		case "/AllPrices.json.xz":
			w.Write(xzBuf.Bytes())
		default:
			http.NotFound(w, r)
		}
	}))
	defer srv.Close()
	c := &Client{BaseURL: srv.URL, HTTP: srv.Client()}

	// TM19 has no file; its tokens come from M19's file.
	ids, err := c.UUIDs([]string{"m19", "TM19", "ZZZ"})
	if err != nil {
		t.Fatal(err)
	}
	if ids["sf-shock"] != "uuid-shock" || ids["sf-angel"] != "uuid-angel" || len(ids) != 2 {
		t.Fatalf("ids = %v", ids)
	}

	var lastRead int64
	prices, err := c.PriceHistory(map[string]bool{"uuid-shock": true}, func(read, total int64) { lastRead = read })
	if err != nil {
		t.Fatal(err)
	}
	if prices["uuid-shock"]["nonfoil"]["2026-11-07"] != 0.25 {
		t.Errorf("prices = %+v", prices)
	}
	if lastRead != int64(xzBuf.Len()) {
		t.Errorf("progress should reach the full download: %d of %d", lastRead, xzBuf.Len())
	}
}
