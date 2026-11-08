package web

import (
	"bytes"
	"encoding/json"
	"io"
	"mime/multipart"
	"net/http"
	"net/http/httptest"
	"net/url"
	"path/filepath"
	"strings"
	"testing"

	"github.com/jrhorner1/mtgmanager/internal/collection"
	"github.com/jrhorner1/mtgmanager/internal/games/mtg"
	"github.com/jrhorner1/mtgmanager/internal/games/mtg/scryfall/scryfalltest"
	"github.com/jrhorner1/mtgmanager/internal/service"
)

func newTestServer(t *testing.T) (*httptest.Server, *service.Game) {
	t.Helper()
	g := &mtg.Game{Scryfall: scryfalltest.NewSearchServer(t)}
	store, err := collection.OpenStore(filepath.Join(t.TempDir(), "collection.json"), mtg.ID)
	if err != nil {
		t.Fatal(err)
	}
	shock := collection.Card{ID: "aaaaaaaa-0000-4000-8000-000000000001", Name: "Shock", Set: "M19", SetName: "Core Set 2019",
		Number: "156", Rarity: "common", ReleasedAt: "2018-07-13", Finishes: []string{"nonfoil", "foil"}, Finish: "nonfoil",
		Condition: collection.ConditionNearMint, Language: "en", Quantity: 2, PurchasePrice: 0.25, AcquiredDate: "2018-08-08",
		Market: map[string]float64{"nonfoil": 0.20, "foil": 1.00}, PricesUpdated: "2026-11-07", TypeLine: "Instant", Text: "Shock deals 2 damage."}
	shock.SetAttr("colors", []string{"R"})
	shock.SetAttr("mana_cost", "{R}")
	opt := collection.Card{ID: "aaaaaaaa-0000-4000-8000-000000000002", Name: "Opt", Set: "DOM", SetName: "Dominaria",
		Number: "60", Rarity: "common", ReleasedAt: "2018-04-27", Finish: "foil", Condition: collection.ConditionLightPlay, Language: "en", Quantity: 1, TypeLine: "Instant"}
	opt.SetAttr("colors", []string{"U"})
	if _, _, err := store.AddCards([]collection.Card{shock, opt}); err != nil {
		t.Fatal(err)
	}
	svc := service.FromStore(g, store)
	srv := httptest.NewServer(New("MTG Manager", "test", svc))
	t.Cleanup(srv.Close)
	return srv, svc
}

func call(t *testing.T, srv *httptest.Server, method, path string, body any, out any) int {
	t.Helper()
	var r io.Reader
	if body != nil {
		b, _ := json.Marshal(body)
		r = bytes.NewReader(b)
	}
	req, _ := http.NewRequest(method, srv.URL+path, r)
	req.Header.Set(ClientHeader, "test")
	resp, err := srv.Client().Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	if out != nil {
		json.NewDecoder(resp.Body).Decode(out)
	}
	return resp.StatusCode
}

func TestPagesAndInfo(t *testing.T) {
	srv, _ := newTestServer(t)
	for _, p := range []string{"/", "/app.js", "/app.css", "/icon.png"} {
		resp, err := srv.Client().Get(srv.URL + p)
		if err != nil || resp.StatusCode != 200 {
			t.Fatalf("GET %s: %v %v", p, resp.StatusCode, err)
		}
		resp.Body.Close()
	}
	var info struct {
		Games []struct{ ID, Name string }
	}
	if call(t, srv, "GET", "/api/info", nil, &info); len(info.Games) != 1 || info.Games[0].ID != "mtg" {
		t.Fatalf("info = %+v", info)
	}
	if code := call(t, srv, "GET", "/api/games/pokemon/overview", nil, nil); code != 404 {
		t.Fatalf("unknown game: %d", code)
	}
}

func TestOverviewAndCards(t *testing.T) {
	srv, _ := newTestServer(t)
	var o struct {
		Copies   int
		Market   float64
		Cost     float64
		Chart    struct{ Range string }
		TopCards []map[string]any
	}
	call(t, srv, "GET", "/api/games/mtg/overview?range=30D", nil, &o)
	if o.Copies != 3 || o.Market != 0.40 || o.Cost != 0.50 || o.Chart.Range != "30D" || len(o.TopCards) != 1 {
		t.Fatalf("overview = %+v", o)
	}

	var all, red struct {
		Matched int
		Sets    []struct {
			Code  string
			Cards []map[string]any
		}
	}
	call(t, srv, "GET", "/api/games/mtg/cards", nil, &all)
	if all.Matched != 2 || len(all.Sets) != 2 || all.Sets[0].Code != "M19" {
		t.Fatalf("cards = %+v", all)
	}
	call(t, srv, "GET", "/api/games/mtg/cards?"+url.Values{"f.Color": {"R"}}.Encode(), nil, &red)
	if red.Matched != 1 || red.Sets[0].Cards[0]["name"] != "Shock" || red.Sets[0].Cards[0]["key"] == "" {
		t.Fatalf("color filter = %+v", red)
	}
}

func TestEditAndDeleteCard(t *testing.T) {
	srv, svc := newTestServer(t)
	shockKey := "aaaaaaaa-0000-4000-8000-000000000001|nonfoil|NM|en"

	var detail struct {
		Card    map[string]any
		Details []struct{ Label, Value string }
		Link    struct{ Label, URL string }
	}
	call(t, srv, "GET", "/api/games/mtg/card?key="+url.QueryEscape(shockKey), nil, &detail)
	if detail.Card["name"] != "Shock" || detail.Details[0].Value != "{R}" || !strings.Contains(detail.Link.URL, "scryfall.com/card/m19/156") {
		t.Fatalf("detail = %+v", detail)
	}

	var upd struct {
		Key    string
		Merged bool
	}
	code := call(t, srv, "PUT", "/api/games/mtg/card?key="+url.QueryEscape(shockKey),
		map[string]any{"quantity": 3, "finish": "foil", "condition": "LP", "purchase_price": 1.5, "acquired_date": "2020-01-02"}, &upd)
	if code != 200 || !strings.Contains(upd.Key, "|foil|LP|") {
		t.Fatalf("update: %d %+v", code, upd)
	}
	if code := call(t, srv, "PUT", "/api/games/mtg/card?key="+url.QueryEscape(upd.Key), map[string]any{"quantity": 1, "acquired_date": "March"}, nil); code != 400 {
		t.Fatalf("bad date should be rejected: %d", code)
	}
	if code := call(t, srv, "DELETE", "/api/games/mtg/card?key="+url.QueryEscape(upd.Key), nil, nil); code != 200 {
		t.Fatalf("delete: %d", code)
	}
	if n := len(svc.Store.GetCollection()); n != 1 {
		t.Fatalf("after delete %d entries", n)
	}
}

func TestSearchAndAdd(t *testing.T) {
	srv, svc := newTestServer(t)
	var page struct {
		Cards []json.RawMessage
		Total int
		Next  string
	}
	call(t, srv, "GET", "/api/games/mtg/search?text=bolt", nil, &page)
	if len(page.Cards) != 3 || page.Total != 5 || page.Next == "" {
		t.Fatalf("search = %d cards, total %d, next %q", len(page.Cards), page.Total, page.Next)
	}
	var card collection.Card
	json.Unmarshal(page.Cards[1], &card)
	var added struct{ Created, Merged int }
	code := call(t, srv, "POST", "/api/games/mtg/cards", map[string]any{"card": card, "quantity": 4, "finish": "foil", "condition": "NM"}, &added)
	if code != 200 || added.Created != 1 {
		t.Fatalf("add: %d %+v", code, added)
	}
	found := false
	for _, c := range svc.Store.GetCollection() {
		if c.Name == "Lightning Bolt" && c.Quantity == 4 && c.Finish == "foil" {
			found = true
		}
	}
	if !found {
		t.Fatal("added card not in the collection")
	}
}

// TestExportImportRoundTrip exports the collection and imports it back: every entry
// merges, doubling the quantities.
func TestExportImportRoundTrip(t *testing.T) {
	srv, svc := newTestServer(t)
	resp, err := srv.Client().Get(srv.URL + "/api/games/mtg/export")
	if err != nil {
		t.Fatal(err)
	}
	data, _ := io.ReadAll(resp.Body)
	resp.Body.Close()
	if !strings.Contains(resp.Header.Get("Content-Disposition"), "attachment") || !service.IsAppFile(data) {
		t.Fatalf("export: %q", resp.Header.Get("Content-Disposition"))
	}

	var body bytes.Buffer
	mw := multipart.NewWriter(&body)
	fw, _ := mw.CreateFormFile("file", "backup.json")
	fw.Write(data)
	mw.Close()
	req, _ := http.NewRequest("POST", srv.URL+"/api/games/mtg/import", &body)
	req.Header.Set("Content-Type", mw.FormDataContentType())
	req.Header.Set(ClientHeader, "test")
	resp, err = srv.Client().Do(req)
	if err != nil {
		t.Fatal(err)
	}
	var res struct {
		Copies, Created, Merged int
		Format                  string
	}
	json.NewDecoder(resp.Body).Decode(&res)
	resp.Body.Close()
	if resp.StatusCode != 200 || res.Copies != 3 || res.Created != 0 || res.Merged != 2 || res.Format != service.AppFileFormat {
		t.Fatalf("import: %d %+v", resp.StatusCode, res)
	}
	total := 0
	for _, c := range svc.Store.GetCollection() {
		total += c.Quantity
	}
	if total != 6 {
		t.Fatalf("quantities after re-import = %d, want 6", total)
	}
}

func TestChangesNeedClientHeader(t *testing.T) {
	srv, svc := newTestServer(t)
	// A cross-site form or fetch can't add the header, so it must be refused.
	resp, err := srv.Client().Post(srv.URL+"/api/games/mtg/card?key=x", "text/plain", strings.NewReader("{}"))
	if err != nil {
		t.Fatal(err)
	}
	resp.Body.Close()
	if resp.StatusCode != http.StatusForbidden {
		t.Fatalf("POST without header: %d", resp.StatusCode)
	}
	req, _ := http.NewRequest("DELETE", srv.URL+"/api/games/mtg/card?key="+url.QueryEscape("aaaaaaaa-0000-4000-8000-000000000001|nonfoil|NM|en"), nil)
	resp, _ = srv.Client().Do(req)
	resp.Body.Close()
	if resp.StatusCode != http.StatusForbidden || len(svc.Store.GetCollection()) != 2 {
		t.Fatalf("DELETE without header: %d", resp.StatusCode)
	}
}

func TestLoopbackRejectsOtherHosts(t *testing.T) {
	h := loopbackOnly(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { w.WriteHeader(204) }))
	for host, want := range map[string]int{"localhost:8080": 204, "127.0.0.1:8080": 204, "[::1]:8080": 204, "evil.example:8080": 403, "attacker.com": 403} {
		req := httptest.NewRequest("GET", "/api/info", nil)
		req.Host = host
		rec := httptest.NewRecorder()
		h.ServeHTTP(rec, req)
		if rec.Code != want {
			t.Errorf("Host %s: %d, want %d", host, rec.Code, want)
		}
	}
}

// TestCardWithoutDetails: tokens and emblems have no mana cost, so the game returns no
// extra detail rows; the API must send an empty list, not null, or the page breaks.
func TestCardWithoutDetails(t *testing.T) {
	srv, _ := newTestServer(t)
	resp, err := srv.Client().Get(srv.URL + "/api/games/mtg/card?key=" + url.QueryEscape("aaaaaaaa-0000-4000-8000-000000000002|foil|LP|en"))
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	var raw map[string]json.RawMessage
	json.NewDecoder(resp.Body).Decode(&raw)
	if string(raw["details"]) != "[]" || string(raw["others"]) != "[]" {
		t.Fatalf("details=%s others=%s, want []", raw["details"], raw["others"])
	}
}
