package scryfall

import (
	"encoding/json"
	"os"
	"strings"
	"time"
)

// SetInfo is the subset of a Scryfall set object used for completion stats.
type SetInfo struct {
	Code       string `json:"code"`
	Name       string `json:"name"`
	CardCount  int    `json:"card_count"`
	ReleasedAt string `json:"released_at"`
	SetType    string `json:"set_type"`
}

type setsCache struct {
	Fetched string    `json:"fetched"` // YYYY-MM-DD
	Sets    []SetInfo `json:"sets"`
}

const setsCacheMaxAge = 7 * 24 * time.Hour

// FetchSets downloads every set from Scryfall, keyed by uppercase set code.
func (c *Client) FetchSets() (map[string]SetInfo, error) {
	var resp struct {
		Data []SetInfo `json:"data"`
	}
	if _, err := c.Do("GET", "/sets", nil, &resp, false); err != nil {
		return nil, err
	}
	return indexSets(resp.Data), nil
}

func indexSets(list []SetInfo) map[string]SetInfo {
	m := make(map[string]SetInfo, len(list))
	for _, s := range list {
		m[strings.ToUpper(s.Code)] = s
	}
	return m
}

// LoadSets returns set metadata from the cache file at path, refreshing it from
// Scryfall when it is missing or older than a week. If Scryfall is unreachable a
// stale cache is still returned.
func LoadSets(c *Client, path string) (map[string]SetInfo, error) {
	var cache setsCache
	if b, err := os.ReadFile(path); err == nil && json.Unmarshal(b, &cache) == nil && len(cache.Sets) > 0 {
		if t, err := time.Parse("2006-01-02", cache.Fetched); err == nil && time.Since(t) < setsCacheMaxAge {
			return indexSets(cache.Sets), nil
		}
	}
	fresh, err := c.FetchSets()
	if err != nil {
		if len(cache.Sets) > 0 {
			return indexSets(cache.Sets), nil
		}
		return nil, err
	}
	cache = setsCache{Fetched: time.Now().Format("2006-01-02")}
	for _, s := range fresh {
		cache.Sets = append(cache.Sets, s)
	}
	if b, err := json.Marshal(cache); err == nil {
		_ = os.WriteFile(path, b, 0o644)
	}
	return fresh, nil
}
