// Package game defines what a trading card game provides to the app. Each game lives
// in its own package under internal/games and keeps its own data folder, so games
// never share files.
package game

import (
	"net/url"

	"github.com/jrhorner1/tcgbinder/internal/collection"
	"github.com/jrhorner1/tcgbinder/internal/scan"
)

// Game is a trading card game the app can manage.
type Game interface {
	// ID is a short stable identifier, used for the game's data folder (e.g. "mtg").
	ID() string
	// Name is the display name (e.g. "Magic: The Gathering").
	Name() string

	// Open prepares the game to use dir, its private data folder, for caches.
	Open(dir string) error

	// Catalog lookups. Returned cards carry catalog data only; ownership fields
	// (finish, condition, quantity, price paid, date) are left for the caller.
	Search(text, set, nextPage string) (SearchPage, error)
	SearchHints() (text, set string) // placeholder text for the Add dialog
	Refresh(ids []string, progress func(done, total int)) (map[string]collection.CatalogUpdate, error)
	Sets() (map[string]collection.SetInfo, error) // keyed by uppercase set code

	// Import reads another app's export (CSV) and matches it to catalog printings.
	Import(data []byte, progress func(done, total int)) ([]collection.Card, ImportReport, error)

	// Printings returns every printing of the same card (e.g. for moving a card to
	// a different set), newest first.
	Printings(c collection.Card) ([]collection.Card, error)
	// SetCards lists every card in a set (by uppercase code), in collector-number order.
	SetCards(code string) ([]collection.Card, error)

	// Presentation.
	Finishes() []string              // finish IDs as stored, e.g. "nonfoil"
	FinishName(finish string) string // display name, e.g. "Normal"
	Rarities() []string              // rarity filter options, lowest first
	Filters() []Filter               // extra filters shown with set/rarity/finish
	Details(c collection.Card) []Row // extra rows in the card editor
	CardLink(c collection.Card) (label string, link *url.URL)
}

// PriceHistorySource is implemented by games that can load daily price history (Tools → Load price history).
type PriceHistorySource interface {
	// PriceHistory returns daily prices for the cards: ID -> finish -> date -> USD.
	PriceHistory(cards []collection.Card, status func(string)) (map[string]map[string]map[string]float64, error)
	// PriceHistoryInfo describes the source for the user, e.g. its size and window.
	PriceHistoryInfo() string
}

// Scanner is implemented by games that can recognize a card from a photo: the text
// read off the card (its name, set code, collector number, ...) is looked up in the
// game's catalog.
type Scanner interface {
	// ScanKey summarizes what a reading says about the card, e.g. "shock|m19|156",
	// and whether it says enough to look the card up. The camera captures once two
	// readings in a row give the same complete key, so a blurry frame isn't used.
	ScanKey(r scan.Reading) (key string, complete bool)
	// Identify looks up the card a reading describes.
	Identify(r scan.Reading) (ScanResult, error)
}

// ScanResult is the outcome of identifying a scanned card.
type ScanResult struct {
	// Read describes what was read off the card, e.g. "Shock · M19 · 156".
	Read string
	// Name is the card name read (or matched), for searching by hand.
	Name string
	// Candidates are the printings it could be, best first. Empty if nothing matched.
	Candidates []collection.Card
	// Exact reports that the first candidate is certainly the printing scanned (its
	// set and collector number were read and agree with the name).
	Exact bool
	// Note explains a partial match, e.g. that the set couldn't be read.
	Note string
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
