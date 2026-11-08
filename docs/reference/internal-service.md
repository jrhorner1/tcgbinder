# `internal/service`

```
package service // import "github.com/jrhorner1/tcgbinder/internal/service"

Package service is the app's logic for one game's collection, shared by the
desktop UI and the web server: opening its files, importing, refreshing prices,
price history and the daily sync.

CONSTANTS

const AppFileFormat = "TCG Binder collection file"
    AppFileFormat names collection files written by this app in import reports.

const DefaultChartRange = "90D"
    DefaultChartRange matches the 90 days of history MTGJSON provides.

const PriceCheckInterval = time.Hour
    PriceCheckInterval is how often running apps check whether prices need
    refreshing. Prices refresh at most once a day, when the stored ones are from
    an earlier day.


VARIABLES

var ChartRanges = []string{"30D", "90D", "1Y", "All"}
    ChartRanges are the Value Over Time ranges offered by the UIs.


FUNCTIONS

func ClipStep(points []collection.TimePoint, from time.Time) []collection.TimePoint
    ClipStep limits a step series to dates on or after from, starting with the
    value in effect on that day so the line doesn't begin at zero mid-range.

func EndAt(points []collection.TimePoint, until time.Time) []collection.TimePoint
    EndAt keeps a step series' points before until and ends it with the value in
    effect on that day.

func IsAppFile(data []byte) bool
    IsAppFile reports whether data looks like a JSON collection file rather than
    CSV.

func ParseImport(g game.Game, data []byte, progress func(done, total int)) ([]collection.Card, game.ImportReport, error)
    ParseImport turns a file into cards for g without saving them. App
    collection files already carry exact catalog IDs and prices, so they are
    used as-is; anything else goes to the game's CSV importer.

func RangeStart(r string, today time.Time) time.Time
    RangeStart is the first day shown for a range; zero means "everything".

func Today() time.Time
    Today is the current date at midnight UTC, the end of every chart.


TYPES

type Game struct {
	Game   game.Game
	Store  *collection.Store
	Prices *collection.PriceHistory
	// Settings, if set, returns the app settings (price refresh frequency, automatic
	// price history).
	Settings func() settings.Settings

	// Has unexported fields.
}
    Game is one game with its open collection and price history.

func FromStore(g game.Game, store *collection.Store) *Game
    FromStore wraps an open store, loading the price history kept next to it.

func Open(root string, g game.Game) (*Game, error)
    Open prepares g's folder inside root and opens its collection and price
    history.

func (s *Game) AutoLoadHistoryDue() bool
    AutoLoadHistoryDue reports whether to load price history without being
    asked: the game offers it, it was never loaded, there are cards, and the
    setting allows it.

func (s *Game) CachedSets() map[string]collection.SetInfo
    CachedSets returns set sizes if already loaded, else nil.

func (s *Game) DailySync()
    DailySync is the startup routine: load price history the first time (when
    allowed), refresh prices when due, and record today's value.

func (s *Game) Import(data []byte, progress func(done, total int)) (ImportResult, error)
    Import adds a file's cards: a CSV export from another app, or a collection
    file written by this app. Cards for a printing already owned add to its
    quantity.

func (s *Game) LoadPriceHistory(status func(string)) (int, error)
    LoadPriceHistory merges the game's daily price history (keeping prices
    already recorded) and returns how many printings had history.

func (s *Game) PriceHistorySource() (game.PriceHistorySource, bool)
    PriceHistorySource reports whether the game offers price history.

func (s *Game) PricesDue() bool
    PricesDue reports whether automatic updates should refresh prices now:
    they are from an earlier day (daily), a week or more old (weekly), or never
    (manual).

func (s *Game) RecordToday() error
    RecordToday saves today's value snapshot and current prices to the
    histories.

func (s *Game) RefreshPrices(progress func(done, total int)) (int, error)
    RefreshPrices fetches current prices from the game's catalog, saves them and
    records today's prices. It returns the number of cards updated.

func (s *Game) RunPriceSchedule(stop <-chan struct{})
    RunPriceSchedule refreshes prices whenever they are due, checking every
    PriceCheckInterval until stop is closed. Used by the web server.

func (s *Game) Sets() (map[string]collection.SetInfo, error)
    Sets returns set sizes for completion stats, loading them once.

type ImportResult struct {
	Report                        game.ImportReport
	Cards, NewCards, AlreadyOwned int
}
    ImportResult summarizes an import.

func (r ImportResult) Summary() string
    Summary describes the result for people.

type ValueChart struct {
	From, To     time.Time              // From is zero for "All"
	HistoryStart time.Time              // first day with recorded prices; zero if none
	MarketValue  []collection.TimePoint // real daily market value
	PricePaid    []collection.TimePoint // running price paid (step line)
	// Estimate values cards at today's prices before recorded prices begin (step
	// line); nil when the real line covers the whole range.
	Estimate []collection.TimePoint
}
    ValueChart is the data behind the Value Over Time chart.

func BuildValueChart(cards []collection.Card, st collection.Stats, prices *collection.PriceHistory, rangeName string, today time.Time) ValueChart
    BuildValueChart computes the chart for cards over the named range.

```
