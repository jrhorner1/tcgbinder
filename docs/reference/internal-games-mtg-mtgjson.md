# `internal/games/mtg/mtgjson`

```
package mtgjson // import "github.com/jrhorner1/tcgbinder/internal/games/mtg/mtgjson"

Package mtgjson reads daily card price history from MTGJSON
(https://mtgjson.com), which publishes the last 90 days of prices for every card
as open data.

FUNCTIONS

func ParseAllPrices(r io.Reader, want map[string]bool) (map[string]DailyPrices, error)
    ParseAllPrices reads an AllPrices JSON document from r, keeping only wanted
    UUIDs.


TYPES

type Client struct {
	BaseURL string
	HTTP    *http.Client
}
    Client downloads MTGJSON files.

func NewClient() *Client

func (c *Client) PriceHistory(want map[string]bool, progress func(read, total int64)) (map[string]DailyPrices, error)
    PriceHistory downloads AllPrices (about 90 days of daily prices for every
    card) and returns TCGplayer retail prices for the wanted MTGJSON UUIDs.
    progress reports compressed bytes downloaded. The file is streamed,
    so memory stays small.

func (c *Client) UUIDs(setCodes []string) (map[string]string, error)
    UUIDs maps Scryfall IDs to MTGJSON UUIDs by reading the set files for
    setCodes. Token sets such as "TM19" have no file of their own; their cards
    are in the parent set's file ("M19"), which is tried when the token set's
    file is missing.

type DailyPrices map[string]map[string]float64
    DailyPrices maps a finish ("nonfoil", "foil", "etched") to date (YYYY-MM-DD)
    to USD.

```
