package scryfall

import (
	"encoding/json"
	"os"
	"testing"
)

// TestDecodeSearchResponse checks the response and Card structs against a real
// Scryfall /cards/search answer.
func TestDecodeSearchResponse(t *testing.T) {
	data, err := os.ReadFile("testdata/search-scapeshift.json")
	if err != nil {
		t.Fatal(err)
	}
	var resp SearchResponse
	if err := json.Unmarshal(data, &resp); err != nil {
		t.Fatal(err)
	}
	if resp.TotalCards != 1 || len(resp.Data) != 1 || resp.HasMore {
		t.Fatalf("response: total=%d len=%d more=%v", resp.TotalCards, len(resp.Data), resp.HasMore)
	}
	c := resp.Data[0]
	if c.Name != "Scapeshift" || c.Set != "m19" || c.Number != "201" || !IsUUID(c.ID) {
		t.Errorf("identity fields: %+v", c)
	}
	if c.Prices.USD == "" || c.ImageURL() == "" || c.Text() == "" || len(c.Finishes) == 0 {
		t.Errorf("prices/image/text/finishes missing: %+v", c)
	}
}
