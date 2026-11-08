# `internal/games/mtg/scryfall`

```
package scryfall // import "github.com/jrhorner1/tcgbinder/internal/games/mtg/scryfall"

Package scryfall is a small, rate-limited client for the Scryfall API
(https://scryfall.com/docs/api): card lookup, search, set listings and set
metadata.

CONSTANTS

const CollectionBatchSize = 75

FUNCTIONS

func BuildSearchQuery(text, setCode string) (string, error)
    BuildSearchQuery combines free text with an optional set code into Scryfall
    syntax. Every printing is returned (unique=prints is set by SearchCards),
    so a set code narrows the results to that set's printing(s).

func IsUUID(s string) bool
func LoadSets(c *Client, path string) (map[string]SetInfo, error)
    LoadSets returns set metadata from the cache file at path, refreshing
    it from Scryfall when it is missing or older than a week. If Scryfall is
    unreachable a stale cache is still returned.


TYPES

type Card struct {
	ID            string    `json:"id"`
	OracleID      string    `json:"oracle_id"`
	MultiverseIDs []int     `json:"multiverse_ids"`
	MTGOID        int       `json:"mtgo_id"`
	ArenaID       int       `json:"arena_id"`
	TCGPlayerID   int       `json:"tcgplayer_id"`
	CardMarketID  int       `json:"cardmarket_id"`
	Name          string    `json:"name"`
	Language      string    `json:"lang"`
	ReleasedAt    string    `json:"released_at"`
	OracleText    string    `json:"oracle_text"`
	TypeLine      string    `json:"type_line"`
	CMC           float64   `json:"cmc"`
	ImageURIs     imageURIs `json:"image_uris"`
	CardFaces     []struct {
		Name       string    `json:"name"`
		ManaCost   string    `json:"mana_cost"`
		OracleText string    `json:"oracle_text"`
		Colors     []string  `json:"colors"`
		ImageURIs  imageURIs `json:"image_uris"`
	} `json:"card_faces"`
	ManaCost string   `json:"mana_cost"`
	Colors   []string `json:"colors"`
	ColorID  []string `json:"color_identity"`
	Finishes []string `json:"finishes"`
	SetID    string   `json:"set_id"`
	Set      string   `json:"set"`
	SetName  string   `json:"set_name"`
	Number   string   `json:"collector_number"`
	Rarity   string   `json:"rarity"`
	Prices   Prices   `json:"prices"`
}
    ScryfallCard is the subset of a Scryfall card object the app uses.

func (sc *Card) ImageURL() string
    ImageURL returns the card's image, falling back to the front face for
    double-faced cards.

func (sc *Card) Text() string
    Text returns the rules text, joining faces for multi-faced cards.

type Client struct {
	BaseURL string
	HTTP    *http.Client
	FastGap time.Duration // minimum gap between ordinary requests
	SlowGap time.Duration // minimum gap between /cards/collection requests

	// Has unexported fields.
}
    ScryfallClient talks to the Scryfall API while respecting its published
    rate limits (https://scryfall.com/docs/api/rate-limits): at most ~10
    requests/second overall and 2/second for the collection and search
    endpoints.

func NewClient() *Client

func (c *Client) CardByNumber(set, number, lang string) (card *Card, found bool, err error)
    CardByNumber fetches the printing with a set code and collector number,
    in a language (Scryfall code, e.g. "ja"; "" for English). found is false on
    a 404.

func (c *Client) Do(method, path string, body any, out any, slow bool) (int, error)
    Do performs a request and decodes a 200 response into out. It returns the
    HTTP status so callers can treat 404 as "not found" rather than an error.

func (c *Client) FetchCollection(bodies []map[string]any) (CollectionResponse, error)
    FetchCollection calls /cards/collection. Scryfall rejects a whole batch
    with 400 if any one identifier is malformed, so on 400 the batch is split to
    isolate the bad identifier and the rest still resolve.

func (c *Client) FetchSets() (map[string]SetInfo, error)
    FetchSets downloads every set from Scryfall, keyed by uppercase set code.

func (c *Client) GetCard(id string) (*Card, error)
    GetCard fetches one printing by Scryfall ID.

func (c *Client) NamedFuzzy(name string) (card *Card, found bool, err error)
    NamedFuzzy finds a card by an approximate name, tolerating typos (Scryfall's
    /cards/named?fuzzy=). found is false when nothing (or more than one card)
    matches.

func (c *Client) SearchAll(query string, maxPages int) ([]Card, error)
    SearchAll returns every printing matching query in collector-number order,
    following all result pages (up to maxPages of 175 cards). Extras and
    variations are included, so a set query lists every card Scryfall counts in
    the set.

func (c *Client) SearchCards(query, nextPage string) (SearchPage, error)
    SearchCards runs a Scryfall search returning every printing, ordered by
    name and then newest first. Pass the previous page's NextPage to continue;
    a search with no matches returns an empty page, not an error.

type CollectionResponse struct {
	Data     []Card           `json:"data"`
	NotFound []map[string]any `json:"not_found"`
}

type Prices struct {
	USD       string `json:"usd,omitempty"`
	USDFoil   string `json:"usd_foil,omitempty"`
	USDEtched string `json:"usd_etched,omitempty"`
}
    CardPrices mirrors Scryfall's prices object (values are decimal strings;
    "" when unknown).

type SearchPage struct {
	Cards    []Card
	Total    int
	NextPage string // full URL of the next page, "" when this is the last
}
    SearchPage is one page of /cards/search results.

type SearchResponse struct {
	Object     string `json:"object"`
	TotalCards int    `json:"total_cards"`
	HasMore    bool   `json:"has_more"`
	NextPage   string `json:"next_page"`
	Data       []Card `json:"data"`
}

type SetInfo struct {
	Code       string `json:"code"`
	Name       string `json:"name"`
	CardCount  int    `json:"card_count"`
	ReleasedAt string `json:"released_at"`
}
    SetInfo is the subset of a Scryfall set object used for completion stats.

```
