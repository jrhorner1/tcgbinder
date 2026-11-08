// Package scryfall is a small, rate-limited client for the Scryfall API
// (https://scryfall.com/docs/api): card lookup, search, sets and image caching.
package scryfall

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
	"sync"
	"time"
)

// ScryfallClient talks to the Scryfall API while respecting its published rate limits
// (https://scryfall.com/docs/api/rate-limits): at most ~10 requests/second overall and
// 2/second for the collection and search endpoints.
type Client struct {
	BaseURL  string
	HTTP     *http.Client
	FastGap  time.Duration // minimum gap between ordinary requests
	SlowGap  time.Duration // minimum gap between /cards/collection requests
	mu       sync.Mutex
	lastCall time.Time
	lastSlow time.Time
}

const userAgent = "MTGLedger/0.1 (+https://github.com/jrhorner1/mtgmanager)"

func NewClient() *Client {
	return &Client{
		BaseURL: "https://api.scryfall.com",
		HTTP:    &http.Client{Timeout: 30 * time.Second},
		FastGap: 110 * time.Millisecond,
		SlowGap: 550 * time.Millisecond,
	}
}

func (c *Client) wait(slow bool) {
	c.mu.Lock()
	defer c.mu.Unlock()
	next := c.lastCall.Add(c.FastGap)
	if slow {
		if s := c.lastSlow.Add(c.SlowGap); s.After(next) {
			next = s
		}
	}
	if d := time.Until(next); d > 0 {
		time.Sleep(d)
	}
	c.lastCall = time.Now()
	if slow {
		c.lastSlow = c.lastCall
	}
}

// Do performs a request and decodes a 200 response into out. It returns the HTTP
// status so callers can treat 404 as "not found" rather than an error.
func (c *Client) Do(method, path string, body any, out any, slow bool) (int, error) {
	var payload []byte
	if body != nil {
		var err error
		if payload, err = json.Marshal(body); err != nil {
			return 0, err
		}
	}
	for attempt := 0; ; attempt++ {
		c.wait(slow)
		req, err := http.NewRequest(method, c.BaseURL+path, bytes.NewReader(payload))
		if err != nil {
			return 0, err
		}
		req.Header.Set("User-Agent", userAgent)
		req.Header.Set("Accept", "application/json")
		if body != nil {
			req.Header.Set("Content-Type", "application/json")
		}
		resp, err := c.HTTP.Do(req)
		if err != nil {
			if attempt < 2 {
				time.Sleep(time.Second)
				continue
			}
			return 0, err
		}
		if (resp.StatusCode == http.StatusTooManyRequests || resp.StatusCode >= 500) && attempt < 3 {
			resp.Body.Close()
			time.Sleep(time.Duration(attempt+1) * time.Second)
			continue
		}
		defer resp.Body.Close()
		if resp.StatusCode != http.StatusOK {
			msg, _ := io.ReadAll(io.LimitReader(resp.Body, 512))
			return resp.StatusCode, fmt.Errorf("scryfall %s %s: %s %s", method, path, resp.Status, strings.TrimSpace(string(msg)))
		}
		return resp.StatusCode, json.NewDecoder(resp.Body).Decode(out)
	}
}
