// Package game defines what a trading card game provides to the app. Each game lives
// in its own package under internal/games and keeps its own data folder, so games
// never share files.
package game

import (
	"net/url"

	"github.com/jrhorner1/mtgmanager/internal/collection"
)

// Game is a trading card game the app can manage.
type Game interface {
	// ID is a short stable identifier, used for the game's data folder (e.g. "mtg").
	ID() string
	// Name is the display name (e.g. "Magic: The Gathering").
	Name() string

	// Open prepares the game to use dir, its private data folder, for caches.
	Open(dir string) error
	// Upgrade converts entries saved by older versions of this game's code. It returns
	// true if the card changed.
	Upgrade(c *collection.Card) bool

	// Catalog lookups. Returned cards carry catalog data only; ownership fields
	// (finish, condition, quantity, price paid, date) are left for the caller.
	Search(text, set, nextPage string) (SearchPage, error)
	SearchHints() (text, set string) // placeholder text for the Add dialog
	Refresh(ids []string, progress func(done, total int)) (map[string]collection.CatalogUpdate, error)
	CardText(id string) (typeLine, text string, err error)
	Sets() (map[string]collection.SetInfo, error) // keyed by uppercase set code

	// Import reads another app's export (CSV) and matches it to catalog printings.
	Import(data []byte, progress func(done, total int)) ([]collection.Card, ImportReport, error)

	// Presentation.
	Finishes() []string              // finish filter options
	Rarities() []string              // rarity filter options, lowest first
	Filters() []Filter               // extra filters shown with set/rarity/finish
	Details(c collection.Card) []Row // extra rows in the card editor
	CardLink(c collection.Card) (label string, link *url.URL)
}

// PriceHistorySource is implemented by games that can backfill daily prices.
type PriceHistorySource interface {
	// PriceHistory returns daily prices for the cards: ID -> finish -> date -> USD.
	PriceHistory(cards []collection.Card, status func(string)) (map[string]map[string]map[string]float64, error)
	// PriceHistoryInfo describes the source for the user, e.g. its size and window.
	PriceHistoryInfo() string
}

// SearchPage is one page of catalog search results.
type SearchPage struct {
	Cards    []collection.Card
	Total    int
	NextPage string // opaque token for the next page; "" on the last
}

// ImportReport summarizes an import for the user.
type ImportReport struct {
	Format string   // detected source app, e.g. "EchoMTG"
	Rows   int      // rows read from the file
	Notes  []string // rows needing attention (approximate matches, not found)
}

// Filter is a game-specific filter menu, e.g. Magic's colors.
type Filter struct {
	Name    string   // shown as the menu's placeholder
	Options []string // choices; the app adds "Any"
	Match   func(c collection.Card, option string) bool
}

// Row is a label/value line in the card editor.
type Row struct {
	Label, Value string
}
