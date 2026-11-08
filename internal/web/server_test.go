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

	"github.com/jrhorner1/tcgbinder/internal/collection"
	"github.com/jrhorner1/tcgbinder/internal/games/mtg"
	"github.com/jrhorner1/tcgbinder/internal/games/mtg/scryfall/scryfalltest"
	"github.com/jrhorner1/tcgbinder/internal/service"
)

func newTestServer(t *testing.T) (*httptest.Server, *service.Game) {
	t.Helper()
	g := &mtg.Game{Scryfall: scryfalltest.NewSearchServer(t)}
	store, err := collection.OpenStore(filepath.Join(t.TempDir(), "collection.json"), mtg.ID)
	if err != nil {
		t.Fatal(err)
	}
	shock := held(collection.Card{ID: "aaaaaaaa-0000-4000-8000-000000000001", Name: "Shock", Set: "M19", SetName: "Core Set 2019",
		CollectorNumber: "156", Rarity: "common", ReleasedAt: "2018-07-13", Finishes: []string{"nonfoil", "foil"}, Language: "en",
		Market: map[string]float64{"nonfoil": 0.20, "foil": 1.00}, PricesUpdated: "2026-11-07", TypeLine: "Instant", RulesText: "Shock deals 2 damage."}, "nonfoil", collection.ConditionNearMint, collection.Purchase{Quantity: 2, PricePaid: 0.25, AcquiredDate: "2018-08-08"})
	shock.SetAttr("colors", []string{"R"})
	shock.SetAttr("mana_cost", "{R}")
	shock.SetAttr("oracle_id", "o-shock") // printings are looked up by Oracle ID
	opt := held(collection.Card{ID: "aaaaaaaa-0000-4000-8000-000000000002", Name: "Opt", Set: "DOM", SetName: "Dominaria",
		CollectorNumber: "60", Rarity: "common", ReleasedAt: "2018-04-27", Language: "en", TypeLine: "Instant"}, "foil", collection.ConditionLightPlay, collection.Purchase{Quantity: 1})
	opt.SetAttr("colors", []string{"U"})
	if _, _, err := store.AddCards([]collection.Card{shock, opt}); err != nil {
		t.Fatal(err)
	}
	svc := service.FromStore(g, store)
	srv := httptest.NewServer(New("TCG Binder", "test", nil, svc))
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
	if code := call(t, srv, "GET", "/api/games/pokemon/home", nil, nil); code != 404 {
		t.Fatalf("unknown game: %d", code)
	}
}

func TestHomeAndCards(t *testing.T) {
	srv, _ := newTestServer(t)
	var o struct {
		Cards       int
		MarketValue float64
		PricePaid   float64
		Chart       struct{ Range string }
		TopCards    []map[string]any
	}
	call(t, srv, "GET", "/api/games/mtg/home?range=30D", nil, &o)
	if o.Cards != 3 || o.MarketValue != 0.40 || o.PricePaid != 0.50 || o.Chart.Range != "30D" || len(o.TopCards) != 1 {
		t.Fatalf("home = %+v", o)
	}

	var all, red struct {
		Matched int
		Sets    int
		Cards   []map[string]any
	}
	call(t, srv, "GET", "/api/games/mtg/cards", nil, &all)
	if all.Matched != 2 || all.Sets != 2 || len(all.Cards) != 2 || all.Cards[0]["name"] != "Opt" { // by name
		t.Fatalf("cards = %+v", all)
	}
	call(t, srv, "GET", "/api/games/mtg/cards?"+url.Values{"f.Color": {"R"}}.Encode(), nil, &red)
	if red.Matched != 1 || red.Cards[0]["name"] != "Shock" || red.Cards[0]["key"] == "" {
		t.Fatalf("color filter = %+v", red)
	}
}

func TestEditAndDeleteCard(t *testing.T) {
	srv, svc := newTestServer(t)
	shockKey := "aaaaaaaa-0000-4000-8000-000000000001"

	var detail struct {
		Card    map[string]any
		Details []struct{ Label, Value string }
		Link    struct{ Label, URL string }
	}
	call(t, srv, "GET", "/api/games/mtg/card?key="+url.QueryEscape(shockKey), nil, &detail)
	if detail.Card["name"] != "Shock" || detail.Details[0].Value != "{R}" || !strings.Contains(detail.Link.URL, "scryfall.com/card/m19/156") {
		t.Fatalf("detail = %+v", detail)
	}

	if purchases, _ := detail.Card["purchases"].([]any); len(purchases) != 1 || detail.Card["copy_count"] != 2.0 {
		t.Fatalf("detail purchases = %+v", detail.Card)
	}

	var upd struct {
		Key    string
		Merged bool
	}
	code := call(t, srv, "PUT", "/api/games/mtg/card?key="+url.QueryEscape(shockKey), map[string]any{"purchases": []map[string]any{
		{"quantity": 1, "finish": "nonfoil", "condition": "NM", "price_paid": 0.10, "acquired_date": "2018-08-08"},
		{"quantity": 1, "finish": "nonfoil", "condition": "NM", "price_paid": 0.12, "acquired_date": "2019-01-02"},
		{"quantity": 3, "finish": "foil", "condition": "LP", "price_paid": 1.5, "acquired_date": "2020-01-02"},
	}}, &upd)
	if code != 200 || upd.Key != shockKey {
		t.Fatalf("update: %d %+v", code, upd)
	}
	for _, c := range svc.Store.GetCollection() {
		if c.ID == shockKey && (c.CopyCount() != 5 || len(c.Purchases("nonfoil", collection.ConditionNearMint)) != 2 || c.ConditionCopyCount("foil", collection.ConditionLightPlay) != 3) {
			t.Fatalf("copies after update = %+v", c.Copies)
		}
	}
	for _, bad := range []map[string]any{
		{"quantity": 1, "acquired_date": "March"},
		{"quantity": 1, "finish": "etched"}, // Shock has no etched printing
	} {
		if code := call(t, srv, "PUT", "/api/games/mtg/card?key="+url.QueryEscape(upd.Key), map[string]any{"purchases": []any{bad}}, nil); code != 400 {
			t.Fatalf("%v should be rejected: %d", bad, code)
		}
	}
	if code := call(t, srv, "DELETE", "/api/games/mtg/card?key="+url.QueryEscape(upd.Key), nil, nil); code != 200 {
		t.Fatalf("delete: %d", code)
	}
	if n := len(svc.Store.GetCollection()); n != 1 {
		t.Fatalf("after delete %d cards", n)
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
	var added struct{ NewCards, AlreadyOwned int }
	code := call(t, srv, "POST", "/api/games/mtg/cards", map[string]any{"card": card, "quantity": 4, "finish": "foil", "condition": "NM"}, &added)
	if code != 200 || added.NewCards != 1 {
		t.Fatalf("add: %d %+v", code, added)
	}
	found := false
	for _, c := range svc.Store.GetCollection() {
		if c.Name == "Lightning Bolt" && c.CopyCount() == 4 && c.FinishCopyCount("foil") == 4 {
			found = true
		}
	}
	if !found {
		t.Fatal("added card not in the collection")
	}
}

// TestExportImportRoundTrip exports the collection and imports it back: every card
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
		Cards, NewCards, AlreadyOwned int
		Format                        string
	}
	json.NewDecoder(resp.Body).Decode(&res)
	resp.Body.Close()
	if resp.StatusCode != 200 || res.Cards != 3 || res.NewCards != 0 || res.AlreadyOwned != 2 || res.Format != service.AppFileFormat {
		t.Fatalf("import: %d %+v", resp.StatusCode, res)
	}
	total := 0
	for _, c := range svc.Store.GetCollection() {
		total += c.CopyCount()
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
	req, _ := http.NewRequest("DELETE", srv.URL+"/api/games/mtg/card?key="+url.QueryEscape("aaaaaaaa-0000-4000-8000-000000000001"), nil)
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
	resp, err := srv.Client().Get(srv.URL + "/api/games/mtg/card?key=" + url.QueryEscape("aaaaaaaa-0000-4000-8000-000000000002"))
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

func TestSetPageAndMissing(t *testing.T) {
	srv, _ := newTestServer(t)
	var set struct {
		Code  string
		Cards int
		Top   []map[string]any
	}
	call(t, srv, "GET", "/api/games/mtg/set?code=M19", nil, &set)
	if set.Code != "M19" || set.Cards != 2 || len(set.Top) != 1 || set.Top[0]["each"] != 0.2 {
		t.Fatalf("set page = %+v", set)
	}
	var m struct {
		Total   int
		Missing []map[string]any
	}
	call(t, srv, "GET", "/api/games/mtg/missing?set=M19", nil, &m)
	// The fake set has 1, 60 and 156; Shock (156) is owned.
	if m.Total != 3 || len(m.Missing) != 2 || m.Missing[0]["collector_number"] != "1" {
		t.Fatalf("missing = %+v", m)
	}
}

func TestChangePrinting(t *testing.T) {
	srv, svc := newTestServer(t)
	shockKey := "aaaaaaaa-0000-4000-8000-000000000001"
	var p struct {
		Current   string
		Printings []map[string]any
	}
	call(t, srv, "GET", "/api/games/mtg/printings?key="+url.QueryEscape(shockKey), nil, &p)
	if len(p.Printings) == 0 {
		t.Fatal("no printings")
	}
	target := p.Printings[0]["id"].(string)
	var res struct{ Key string }
	if code := call(t, srv, "PUT", "/api/games/mtg/card/printing?key="+url.QueryEscape(shockKey), map[string]string{"id": target}, &res); code != 200 {
		t.Fatalf("change printing: %d", code)
	}
	moved := false
	for _, c := range svc.Store.GetCollection() {
		if c.ID == target && c.CopyCount() == 2 && c.PurchaseRows()[0].PricePaid == 0.25 {
			moved = true
		}
	}
	if !moved {
		t.Fatalf("card not moved: %+v", svc.Store.GetCollection())
	}
	if code := call(t, srv, "PUT", "/api/games/mtg/card/printing?key="+url.QueryEscape(res.Key), map[string]string{"id": "not-a-printing"}, nil); code != 400 {
		t.Fatalf("unknown printing should be rejected: %d", code)
	}
}

func TestBulkEditAndSearch(t *testing.T) {
	srv, svc := newTestServer(t)
	keys := []string{}
	for _, c := range svc.Store.GetCollection() {
		keys = append(keys, collection.CardKey(c))
	}
	var res struct {
		Updated int
		Skipped []string
	}
	if code := call(t, srv, "POST", "/api/games/mtg/bulk", map[string]any{"keys": keys, "condition": "HP", "price_paid": 0.5}, &res); code != 200 || res.Updated != 2 {
		t.Fatalf("bulk: %d %+v", code, res)
	}
	for _, c := range svc.Store.GetCollection() {
		for _, l := range c.PurchaseRows() {
			if l.Condition != collection.ConditionHeavyPlay || l.PricePaid != 0.5 {
				t.Fatalf("not applied: %+v", c.Copies)
			}
		}
	}
	if code := call(t, srv, "POST", "/api/games/mtg/bulk", map[string]any{"keys": keys}, nil); code != 400 {
		t.Fatalf("no change should be rejected: %d", code)
	}

	var found struct {
		Games []struct {
			Game  string
			Total int
		}
	}
	call(t, srv, "GET", "/api/search?q=shock", nil, &found)
	if len(found.Games) != 1 || found.Games[0].Total != 1 {
		t.Fatalf("search = %+v", found)
	}
}

// held gives c copies in one finish and condition (test fixtures).
func held(c collection.Card, finish string, cond collection.Condition, l collection.Purchase) collection.Card {
	c.AddPurchase(finish, cond, l)
	return c
}

func TestSettingsAPI(t *testing.T) {
	srv, _ := newTestServer(t)
	var got struct {
		Settings map[string]any
		Choices  map[string][]string
	}
	call(t, srv, "GET", "/api/settings", nil, &got)
	if got.Settings["theme"] != "system" || len(got.Choices["theme"]) != 3 {
		t.Fatalf("settings = %+v", got)
	}
	// Only the settings sent change.
	if code := call(t, srv, "PUT", "/api/settings", map[string]any{"theme": "dark", "show_images": false}, &got); code != 200 ||
		got.Settings["theme"] != "dark" || got.Settings["show_images"] != false || got.Settings["card_order"] != "name" {
		t.Fatalf("put: %d %+v", code, got.Settings)
	}
	if code := call(t, srv, "PUT", "/api/settings", map[string]any{"theme": "purple"}, nil); code != 400 {
		t.Fatalf("bad theme should be rejected: %d", code)
	}

	// The collection lists cards in the chosen order: by set, newest first, puts Core Set
	// 2019 (2018-07) before Dominaria (2018-04).
	call(t, srv, "PUT", "/api/settings", map[string]any{"card_order": "newest"}, nil)
	var cards struct{ Cards []struct{ Set string } }
	call(t, srv, "GET", "/api/games/mtg/cards", nil, &cards)
	if len(cards.Cards) != 2 || cards.Cards[0].Set != "M19" {
		t.Fatalf("cards by set, newest first = %+v", cards.Cards)
	}
}
