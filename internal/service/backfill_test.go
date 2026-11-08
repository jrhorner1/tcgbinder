package service

import (
	"bytes"
	"compress/gzip"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"testing"

	"github.com/jrhorner1/mtgmanager/internal/collection"
	"github.com/jrhorner1/mtgmanager/internal/games/mtg"
	"github.com/jrhorner1/mtgmanager/internal/games/mtg/mtgjson"
	"github.com/ulikunitz/xz"
)

func TestBackfillMergesHistory(t *testing.T) {
	var xzBuf bytes.Buffer
	xw, _ := xz.NewWriter(&xzBuf)
	xw.Write([]byte(`{"meta":{},"data":{"u1":{"paper":{"tcgplayer":{"retail":{"normal":{"2026-10-01":0.5,"2026-11-01":0.75}}}}}}}`))
	xw.Close()
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/M19.json.gz":
			zw := gzip.NewWriter(w)
			zw.Write([]byte(`{"data":{"cards":[{"uuid":"u1","identifiers":{"scryfallId":"sf1"}}]}}`))
			zw.Close()
		case "/AllPrices.json.xz":
			w.Write(xzBuf.Bytes())
		default:
			http.NotFound(w, r)
		}
	}))
	defer srv.Close()

	store, _ := collection.OpenStore(filepath.Join(t.TempDir(), "collection.json"), mtg.ID)
	store.AddCards([]collection.Card{{ID: "sf1", Set: "M19", Finish: "nonfoil", Quantity: 2}, {ID: "sf2", Set: "M19", Finish: "nonfoil", Quantity: 1}})
	g := &mtg.Game{MTGJSON: &mtgjson.Client{BaseURL: srv.URL, HTTP: srv.Client()}}
	svc := FromStore(g, store)
	h := svc.Prices
	var msgs []string
	n, err := svc.Backfill(func(m string) { msgs = append(msgs, m) })
	if err != nil || n != 1 {
		t.Fatalf("backfill = %d, %v", n, err)
	}
	if h.Cards["sf1"]["nonfoil"]["2026-11-01"] != 0.75 || h.BackfilledOn() == "" || len(msgs) == 0 {
		t.Fatalf("history = %+v, msgs = %v", h.Cards, msgs)
	}
}
