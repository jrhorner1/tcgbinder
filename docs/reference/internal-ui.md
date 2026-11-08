# `internal/ui`

```
package ui // import "github.com/jrhorner1/tcgbinder/internal/ui"

Package ui is the Fyne user interface: the collection tree, dashboard, card
editor, and add/import dialogs. It works only through the game.Game interface
and never imports a particular game.

TYPES

type App struct {
	// Has unexported fields.
}
    App is the running desktop/mobile UI: collection tree, pages and dialogs.

func New(window fyne.Window, prefs *settings.Store, games ...*service.Game) *App
    New creates the UI for the given games (each already opened, with its own
    files), shown in window, with the user's settings. The first game starts
    active.

func (ui *App) BuildLayout() fyne.CanvasObject
    BuildLayout constructs the main window content: side-by-side tree and page
    on desktop, or a list screen that pushes page screens on phones.

func (ui *App) RefreshDataIndex()
    RefreshDataIndex reloads every game's cards from its store and refreshes the
    view.

func (ui *App) StartBackgroundSync()
    StartBackgroundSync, for every game: loads set sizes, and once a day
    refreshes market prices so the value-over-time chart gains a point each day
    the app is used.

type BarChart struct {
	widget.BaseWidget
	Items []BarItem
	Max   float64 // scale maximum; 0 means the largest item value
	Color color.Color
	// OnTapped, if set, is called with the index of the row tapped.
	OnTapped func(i int)
}

func NewBarChart(items []BarItem, max float64, c color.Color) *BarChart

func (b *BarChart) CreateRenderer() fyne.WidgetRenderer

func (b *BarChart) Cursor() desktop.Cursor
    Cursor shows a pointer over rows that open something.

func (b *BarChart) Tapped(ev *fyne.PointEvent)
    Tapped calls OnTapped with the row under the tap.

type BarItem struct {
	Label     string
	Value     float64
	ValueText string
}

type LineChart struct {
	widget.BaseWidget
	Series  []Series
	FormatY func(float64) string
	Empty   string    // shown when there are no points
	XMin    time.Time // optional fixed date range; zero means fit the data
	XMax    time.Time
}

func NewLineChart(series []Series, formatY func(float64) string) *LineChart

func (l *LineChart) CreateRenderer() fyne.WidgetRenderer

type Series struct {
	Name   string
	Color  color.Color
	Points []collection.TimePoint
	Step   bool // hold each value until the next point (for running totals)
}

```
