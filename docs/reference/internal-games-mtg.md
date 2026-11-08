# `internal/games/mtg`

```
package mtg // import "github.com/jrhorner1/tcgbinder/internal/games/mtg"

Package mtg is Magic: The Gathering for the collection app: Scryfall for the
card catalog and prices, CSV import from other collection apps, and MTGJSON for
daily price history.

CONSTANTS

const ID = "mtg"
    ID is the game's identifier and data folder name.


FUNCTIONS

func FromScryfall(sc *scryfall.Card) collection.Card
    FromScryfall copies a printing's catalog data into a collection card.
    Magic-only fields go into Attrs under the same JSON names earlier versions
    used.


TYPES

type Game struct {
	Scryfall *scryfall.Client
	MTGJSON  *mtgjson.Client

	// Has unexported fields.
}
    Game implements game.Game and game.PriceHistorySource for Magic.

func New() *Game
    New returns Magic using the live Scryfall and MTGJSON services.

func (g *Game) CardLink(c collection.Card) (string, *url.URL)

func (g *Game) Details(c collection.Card) []game.Row

func (g *Game) Filters() []game.Filter

func (g *Game) FinishName(f string) string
    FinishName is the finish as players say it: Scryfall's "nonfoil" is
    "Normal".

func (g *Game) Finishes() []string

func (g *Game) ID() string

func (g *Game) Identify(r scan.Reading) (game.ScanResult, error)
    Identify implements game.Scanner: the printing by set and collector number
    when they were read (checked against the name), otherwise every printing of
    the card whose name is closest to the one read.

func (g *Game) Import(data []byte, progress func(done, total int)) ([]collection.Card, game.ImportReport, error)
    Import reads a CSV export, matches every row to its exact Scryfall printing
    and returns the cards to add.

func (g *Game) Name() string

func (g *Game) Open(dir string) error

func (g *Game) PriceHistory(cards []collection.Card, status func(string)) (map[string]map[string]map[string]float64, error)
    PriceHistory matches the cards' printings to MTGJSON through the per-set
    files and returns MTGJSON's daily prices for them, keyed by Scryfall ID.

func (g *Game) PriceHistoryInfo() string

func (g *Game) Printings(c collection.Card) ([]collection.Card, error)
    Printings finds every printing of c's card by its Oracle ID (or exact name).

func (g *Game) Rarities() []string

func (g *Game) Refresh(ids []string, progress func(done, total int)) (map[string]collection.CatalogUpdate, error)

func (g *Game) ScanKey(r scan.Reading) (string, bool)
    ScanKey implements game.Scanner.

func (g *Game) Search(text, set, nextPage string) (game.SearchPage, error)

func (g *Game) SearchHints() (string, string)

func (g *Game) SetCards(code string) ([]collection.Card, error)
    SetCards lists a set's cards, cached for the session (sets rarely change).

func (g *Game) Sets() (map[string]collection.SetInfo, error)
    Sets loads Scryfall's set list, cached for a week in the game's data folder.

```
