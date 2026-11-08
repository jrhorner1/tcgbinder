# `internal/settings`

```
package settings // import "github.com/jrhorner1/tcgbinder/internal/settings"

Package settings holds the user's app settings (appearance, price updates,
defaults for new cards, ...), shared by the desktop, phone and web apps.
They are stored as settings.json in the data folder, so they move with the
collection.

CONSTANTS

const FileName = "settings.json"
    FileName is the settings file in the data folder.


VARIABLES

var (
	Themes         = []string{"system", "light", "dark"}
	TextSizes      = []string{"small", "normal", "large", "larger"}
	CardOrders     = []string{"name", "newest", "oldest", "value"}
	PriceRefreshes = []string{"daily", "weekly", "manual"}
	ChartRanges    = []string{"30D", "90D", "1Y", "All"}
	Conditions     = []string{"NM", "LP", "MP", "HP", "DMG"}
	DefaultDates   = []string{"today", "none"}
	LogLevels      = []string{"debug", "info", "warn", "error"}
)
    Choices for the settings with a fixed set of values.


TYPES

type Settings struct {
	// Appearance
	Theme      string `json:"theme"`       // system, light or dark
	TextSize   string `json:"text_size"`   // small, normal, large or larger
	ShowImages bool   `json:"show_images"` // download and show card images

	// Collection
	CardOrder  string `json:"card_order"`  // order of the collection's cards: name, by set (newest/oldest), value
	ChartRange string `json:"chart_range"` // value-over-time range shown first

	// Prices
	PriceRefresh     string `json:"price_refresh"`      // daily, weekly or manual
	AutoPriceHistory bool   `json:"auto_price_history"` // load price history automatically the first time (desktop and web)

	// New cards
	DefaultCondition string `json:"default_condition"` // condition preselected when adding a card
	DefaultDate      string `json:"default_date"`      // acquired date when adding a card: today or none

	// Advanced
	ConfirmDelete bool   `json:"confirm_delete"` // ask before deleting cards
	LogLevel      string `json:"log_level"`      // debug, info, warn or error
}
    Settings are the user's choices. Fields missing from the file keep their
    defaults.

func Defaults() Settings
    Defaults are the settings before the user changes anything.

func (s Settings) TextScale() float32
    TextScale is the factor applied to text and spacing for the text size.

func (s Settings) Validate() error
    Validate reports the first value that isn't one of its setting's choices.

type Store struct {
	// Has unexported fields.
}
    Store keeps the settings and saves every change.

func Memory() *Store
    Memory returns a store that isn't saved anywhere (tests).

func Open(dir string) *Store
    Open reads the settings in dir (defaults if there is no file yet). A file
    that can't be read is set aside and the defaults are used, so the app always
    starts.

func (st *Store) Get() Settings
    Get returns the current settings.

func (st *Store) OnChange(f func(old, new Settings))
    OnChange calls f after every change, with the settings before and after.

func (st *Store) Path() string
    Path is the settings file ("" for a memory store).

func (st *Store) Set(s Settings) error
    Set validates and saves new settings, then tells the listeners.

func (st *Store) Update(f func(*Settings)) error
    Update changes some settings: Set(f(current)).

```
