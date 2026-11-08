# `internal/games/mtg/importer`

```
package importer // import "github.com/jrhorner1/tcgbinder/internal/games/mtg/importer"

Package importer reads Magic collection exports from other apps (EchoMTG,
ManaBox, Moxfield, Deckbox, TCGplayer, Dragon Shield, Archidekt and generic CSV)
and matches each row to its exact Scryfall printing.

FUNCTIONS

func ChooseFinish(want string, available []string) string
    ChooseFinish keeps the exported finish when the printing exists in it;
    otherwise it falls back to the printing's only finish (e.g. a foil-only
    promo marked nonfoil).

func ParseFinish(raw string) string
    ParseFinish normalizes foil markers from any export to Scryfall's finish
    names.

func ParseLanguage(raw string) string
    ParseLanguage maps language names/codes to Scryfall's codes, defaulting to
    English.


TYPES

type Resolved struct {
	Row         Row
	Card        *scryfall.Card // nil when unresolved
	Method      string         // identifier kind that produced the match
	Approximate bool           // matched only by name; printing may differ from the one owned
	Err         string         // why it could not be resolved
}
    ResolvedRow is an import row paired with the Scryfall printing it was
    matched to.

func Resolve(c *scryfall.Client, rows []Row, progress func(done, total int)) []Resolved
    ResolveRows matches every row to a Scryfall printing. It tries each row's
    identifiers in order, accepting a match only when the returned card's name
    agrees with the row's name (when the export has one), so a wrong collector
    number in the source file falls through to the next identifier instead of
    importing the wrong card. progress, if set, is called with the number of
    rows settled so far.

type Row struct {
	Line         int // 1-based line in the source file, for error reporting
	Name         string
	SetCode      string // lowercase Scryfall-style set code when known
	SetName      string
	Number       string // collector number
	ScryfallID   string
	MultiverseID int
	MTGOID       int
	TCGPlayerID  int
	Quantity     int
	Finish       string // "nonfoil", "foil" or "etched"
	Condition    collection.Condition
	Language     string // Scryfall language code, e.g. "en", "ja"
	PricePaid    float64
	AcquiredDate string // YYYY-MM-DD or ""
}
    ImportRow is one line of an external collection export, normalized but not
    yet matched to a Scryfall printing.

func ParseCSV(data []byte) (rows []Row, format string, err error)
    ParseImportCSV reads a collection export from any supported app and returns
    its rows.

func (r Row) Describe() string
    Describe is a short human label for reports, e.g. "Line 12: Lightning Bolt
    (2XM #117)".

```
