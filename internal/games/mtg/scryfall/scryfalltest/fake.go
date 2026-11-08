// Package scryfalltest provides fake Scryfall API servers for tests.
package scryfalltest

import (
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strconv"
	"strings"
	"testing"

	"github.com/jrhorner1/tcgbinder/internal/games/mtg/scryfall"
)

// fakeSearch serves /cards/search: "bolt" yields 5 printings in pages of 3; anything
// else is a 404 like Scryfall's no-results answer.
func NewSearchServer(t *testing.T) *scryfall.Client {
	t.Helper()
	var srv *httptest.Server
	srv = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/cards/search" {
			http.NotFound(w, r)
			return
		}
		if r.URL.Query().Get("unique") != "prints" {
			t.Errorf("search should request every printing, got unique=%q", r.URL.Query().Get("unique"))
		}
		q := r.URL.Query().Get("q")
		// "e:<set>" lists that set's cards: Shock (156), Opt (60) and Bolt (1).
		if code, ok := strings.CutPrefix(q, "e:"); ok {
			var data []scryfall.Card
			for _, c := range []struct{ id, name, num string }{
				{"aaaaaaaa-0000-4000-8000-000000000101", "Lightning Bolt", "1"},
				{"aaaaaaaa-0000-4000-8000-000000000060", "Opt", "60"},
				{"aaaaaaaa-0000-4000-8000-000000000001", "Shock", "156"},
			} {
				data = append(data, scryfall.Card{ID: c.id, Name: c.name, Set: code, SetName: strings.ToUpper(code) + " Edition",
					Number: c.num, Rarity: "common", Finishes: []string{"nonfoil", "foil"}, Prices: scryfall.Prices{USD: "0.25"}})
			}
			_ = json.NewEncoder(w).Encode(scryfall.SearchResponse{TotalCards: len(data), Data: data})
			return
		}
		// "oracleid:..." finds every printing of a card: treat it as Lightning Bolt.
		if strings.HasPrefix(q, "oracleid:") || strings.Contains(q, `"Lightning Bolt"`) {
			q = "bolt"
		}
		if !strings.Contains(q, "bolt") {
			w.WriteHeader(http.StatusNotFound)
			_, _ = w.Write([]byte(`{"object":"error","code":"not_found"}`))
			return
		}
		sets := []string{"2xm", "m10", "m11", "a25", "lea"}
		if strings.Contains(q, "set:") {
			sets = []string{q[strings.Index(q, "set:")+4:]}
		}
		page, _ := strconv.Atoi(r.URL.Query().Get("page"))
		if page == 0 {
			page = 1
		}
		start, end := (page-1)*3, page*3
		if end > len(sets) {
			end = len(sets)
		}
		resp := scryfall.SearchResponse{TotalCards: len(sets)}
		for i, s := range sets[start:end] {
			resp.Data = append(resp.Data, scryfall.Card{
				ID: fmt.Sprintf("%s-%d", s, i), Name: "Lightning Bolt", Set: s, SetName: strings.ToUpper(s) + " Edition",
				Number: strconv.Itoa(100 + start + i), Rarity: "common", ReleasedAt: "2020-01-01",
				TypeLine: "Instant", OracleText: "Lightning Bolt deals 3 damage to any target.",
				Finishes: []string{"nonfoil", "foil"}, Prices: scryfall.Prices{USD: "1.50"},
			})
		}
		if end < len(sets) {
			resp.HasMore = true
			v := r.URL.Query()
			v.Set("page", strconv.Itoa(page+1))
			resp.NextPage = srv.URL + "/cards/search?" + v.Encode()
		}
		_ = json.NewEncoder(w).Encode(resp)
	}))
	t.Cleanup(srv.Close)
	c := scryfall.NewClient()
	c.BaseURL = srv.URL
	c.FastGap, c.SlowGap = 0, 0
	return c
}

// fakeScryfall serves /cards/collection from a small catalog, mimicking Scryfall's
// 400-on-malformed-batch behavior and not_found reporting.
func NewCollectionServer(t *testing.T, catalog []scryfall.Card) *scryfall.Client {
	t.Helper()
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/cards/collection" {
			http.NotFound(w, r)
			return
		}
		var req struct {
			Identifiers []map[string]any `json:"identifiers"`
		}
		_ = json.NewDecoder(r.Body).Decode(&req)
		var resp scryfall.CollectionResponse
		for _, id := range req.Identifiers {
			if m, ok := id["multiverse_id"].(float64); ok && m > 999999 {
				w.WriteHeader(http.StatusBadRequest)
				_, _ = w.Write([]byte(`{"details":"multiverse_id too long"}`))
				return
			}
			var hit *scryfall.Card
			for i := range catalog {
				c := &catalog[i]
				switch {
				case id["set"] != nil && id["collector_number"] != nil:
					if c.Set == id["set"] && c.Number == id["collector_number"] {
						hit = c
					}
				case id["multiverse_id"] != nil:
					for _, m := range c.MultiverseIDs {
						if float64(m) == id["multiverse_id"] {
							hit = c
						}
					}
				case id["name"] != nil:
					if strings.EqualFold(frontFace(c.Name), id["name"].(string)) && (id["set"] == nil || c.Set == id["set"]) {
						hit = c
					}
				}
				if hit != nil {
					break
				}
			}
			if hit != nil {
				resp.Data = append(resp.Data, *hit)
			} else {
				resp.NotFound = append(resp.NotFound, id)
			}
		}
		_ = json.NewEncoder(w).Encode(resp)
	}))
	t.Cleanup(srv.Close)
	c := scryfall.NewClient()
	c.BaseURL = srv.URL
	c.FastGap, c.SlowGap = 0, 0
	return c
}

func frontFace(name string) string {
	if i := strings.Index(name, " // "); i > 0 {
		return name[:i]
	}
	return name
}
