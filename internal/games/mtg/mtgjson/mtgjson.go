// Package mtgjson reads daily card price history from MTGJSON (https://mtgjson.com),
// which publishes the last 90 days of prices for every card as open data.
package mtgjson

import (
	"compress/gzip"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"

	"github.com/ulikunitz/xz"
)

const userAgent = "TCGBinder/0.1 (+https://github.com/jrhorner1/tcgbinder)"

// Client downloads MTGJSON files.
type Client struct {
	BaseURL string
	HTTP    *http.Client
}

func NewClient() *Client {
	return &Client{
		BaseURL: "https://mtgjson.com/api/v5",
		HTTP:    &http.Client{Timeout: 15 * time.Minute}, // AllPrices is ~50 MB compressed
	}
}

// DailyPrices maps a finish ("nonfoil", "foil", "etched") to date (YYYY-MM-DD) to USD.
type DailyPrices map[string]map[string]float64

func (c *Client) get(path string) (*http.Response, error) {
	req, err := http.NewRequest("GET", c.BaseURL+"/"+path, nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("User-Agent", userAgent)
	resp, err := c.HTTP.Do(req)
	if err != nil {
		return nil, err
	}
	if resp.StatusCode != http.StatusOK {
		resp.Body.Close()
		return resp, fmt.Errorf("mtgjson %s: %s", path, resp.Status)
	}
	return resp, nil
}

type setFile struct {
	Data struct {
		Code   string     `json:"code"`
		Cards  []setEntry `json:"cards"`
		Tokens []setEntry `json:"tokens"`
	} `json:"data"`
}

type setEntry struct {
	UUID        string `json:"uuid"`
	Identifiers struct {
		ScryfallID string `json:"scryfallId"`
	} `json:"identifiers"`
}

// UUIDs maps Scryfall IDs to MTGJSON UUIDs by reading the set files for setCodes.
// Token sets such as "TM19" have no file of their own; their cards are in the parent
// set's file ("M19"), which is tried when the token set's file is missing.
func (c *Client) UUIDs(setCodes []string) (map[string]string, error) {
	out := map[string]string{}
	fetched := map[string]bool{}
	var firstErr error
	for _, code := range setCodes {
		code = strings.ToUpper(code)
		candidates := []string{code}
		if len(code) > 3 && strings.HasPrefix(code, "T") {
			candidates = append(candidates, code[1:])
		}
		for _, cand := range candidates {
			if fetched[cand] {
				break
			}
			sf, status, err := c.fetchSet(cand)
			if status == http.StatusNotFound {
				continue // try the parent set
			}
			fetched[cand] = true
			if err != nil {
				if firstErr == nil {
					firstErr = err
				}
				break
			}
			for _, e := range append(sf.Data.Cards, sf.Data.Tokens...) {
				if e.Identifiers.ScryfallID != "" {
					out[e.Identifiers.ScryfallID] = e.UUID
				}
			}
			break
		}
	}
	if len(out) == 0 && firstErr != nil {
		return nil, firstErr
	}
	return out, nil
}

func (c *Client) fetchSet(code string) (*setFile, int, error) {
	resp, err := c.get(code + ".json.gz")
	if err != nil {
		if resp != nil {
			return nil, resp.StatusCode, err
		}
		return nil, 0, err
	}
	defer resp.Body.Close()
	gz, err := gzip.NewReader(resp.Body)
	if err != nil {
		return nil, resp.StatusCode, err
	}
	var sf setFile
	if err := json.NewDecoder(gz).Decode(&sf); err != nil {
		return nil, resp.StatusCode, fmt.Errorf("mtgjson %s: %w", code, err)
	}
	return &sf, resp.StatusCode, nil
}

// countingReader reports download progress.
type countingReader struct {
	r        io.Reader
	n, total int64
	progress func(read, total int64)
	last     int64
}

func (cr *countingReader) Read(p []byte) (int, error) {
	n, err := cr.r.Read(p)
	cr.n += int64(n)
	if cr.progress != nil && (cr.n-cr.last > 1<<20 || err == io.EOF) {
		cr.last = cr.n
		cr.progress(cr.n, cr.total)
	}
	return n, err
}

// PriceHistory downloads AllPrices (about 90 days of daily prices for every card) and
// returns TCGplayer retail prices for the wanted MTGJSON UUIDs. progress reports
// compressed bytes downloaded. The file is streamed, so memory stays small.
func (c *Client) PriceHistory(want map[string]bool, progress func(read, total int64)) (map[string]DailyPrices, error) {
	resp, err := c.get("AllPrices.json.xz")
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	cr := &countingReader{r: resp.Body, total: resp.ContentLength, progress: progress}
	xr, err := xz.NewReader(cr)
	if err != nil {
		return nil, fmt.Errorf("AllPrices: %w", err)
	}
	return ParseAllPrices(xr, want)
}

// priceEntry is the part of an AllPrices card entry we use.
type priceEntry struct {
	Paper map[string]struct { // provider -> prices
		Retail map[string]map[string]float64 `json:"retail"` // normal|foil|etched -> date -> price
	} `json:"paper"`
}

// providers in order of preference. TCGplayer matches Scryfall's USD prices; Card
// Kingdom and Mana Pool fill gaps for cards TCGplayer doesn't list.
var providers = []string{"tcgplayer", "cardkingdom", "manapool"}

// ParseAllPrices reads an AllPrices JSON document from r, keeping only wanted UUIDs.
func ParseAllPrices(r io.Reader, want map[string]bool) (map[string]DailyPrices, error) {
	dec := json.NewDecoder(r)
	if err := expectDelim(dec, '{'); err != nil {
		return nil, err
	}
	out := map[string]DailyPrices{}
	for dec.More() {
		key, err := dec.Token()
		if err != nil {
			return nil, err
		}
		if key != "data" {
			var skip json.RawMessage
			if err := dec.Decode(&skip); err != nil {
				return nil, err
			}
			continue
		}
		if err := expectDelim(dec, '{'); err != nil {
			return nil, err
		}
		for dec.More() {
			tok, err := dec.Token()
			if err != nil {
				return nil, err
			}
			uuid, _ := tok.(string)
			if !want[uuid] {
				var skip json.RawMessage
				if err := dec.Decode(&skip); err != nil {
					return nil, err
				}
				continue
			}
			var e priceEntry
			if err := dec.Decode(&e); err != nil {
				return nil, err
			}
			if dp := pickPrices(e); len(dp) > 0 {
				out[uuid] = dp
			}
		}
		if _, err := dec.Token(); err != nil { // closing } of data
			return nil, err
		}
	}
	return out, nil
}

// pickPrices takes each finish from the most preferred provider that has it.
func pickPrices(e priceEntry) DailyPrices {
	dp := DailyPrices{}
	for _, finish := range []struct{ mtgjson, ours string }{{"normal", "nonfoil"}, {"foil", "foil"}, {"etched", "etched"}} {
		for _, p := range providers {
			if series := e.Paper[p].Retail[finish.mtgjson]; len(series) > 0 {
				dp[finish.ours] = series
				break
			}
		}
	}
	return dp
}

func expectDelim(dec *json.Decoder, want json.Delim) error {
	tok, err := dec.Token()
	if err != nil {
		return err
	}
	if d, ok := tok.(json.Delim); !ok || d != want {
		return errors.New("unexpected AllPrices format")
	}
	return nil
}
