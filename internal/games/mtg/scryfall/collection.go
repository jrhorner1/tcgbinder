package scryfall

import (
	"log/slog"
	"net/http"
)

type CollectionResponse struct {
	Data     []Card           `json:"data"`
	NotFound []map[string]any `json:"not_found"`
}

const CollectionBatchSize = 75

// FetchCollection calls /cards/collection. Scryfall rejects a whole batch with 400 if
// any one identifier is malformed, so on 400 the batch is split to isolate the bad
// identifier and the rest still resolve.
func (c *Client) FetchCollection(bodies []map[string]any) (CollectionResponse, error) {
	var resp CollectionResponse
	status, err := c.Do("POST", "/cards/collection", map[string]any{"identifiers": bodies}, &resp, true)
	if err == nil || status != http.StatusBadRequest {
		return resp, err
	}
	if len(bodies) == 1 {
		slog.Warn("scryfall rejected identifier", "identifier", bodies[0], "err", err)
		return CollectionResponse{NotFound: bodies}, nil
	}
	mid := len(bodies) / 2
	left, lerr := c.FetchCollection(bodies[:mid])
	right, rerr := c.FetchCollection(bodies[mid:])
	if lerr != nil {
		return resp, lerr
	}
	if rerr != nil {
		return resp, rerr
	}
	return CollectionResponse{
		Data:     append(left.Data, right.Data...),
		NotFound: append(left.NotFound, right.NotFound...),
	}, nil
}

func IsUUID(s string) bool {
	if len(s) != 36 {
		return false
	}
	for i, r := range s {
		switch i {
		case 8, 13, 18, 23:
			if r != '-' {
				return false
			}
		default:
			if !(r >= '0' && r <= '9' || r >= 'a' && r <= 'f' || r >= 'A' && r <= 'F') {
				return false
			}
		}
	}
	return true
}
