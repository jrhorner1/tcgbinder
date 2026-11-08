package ui

import (
	"fmt"
	"image/color"
	"log/slog"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"github.com/jrhorner1/tcgbinder/internal/collection"
	"github.com/jrhorner1/tcgbinder/internal/game"
	"github.com/jrhorner1/tcgbinder/internal/service"
	"github.com/jrhorner1/tcgbinder/internal/settings"
)

var colorEstimate = color.NRGBA{R: 0x8a, G: 0x93, B: 0xa6, A: 0xff} // muted grey

func (ui *App) chartRange() string {
	return ui.settings.Get().ChartRange
}

// valueSection builds the Value Over Time chart with its range picker and the
// price-history controls.
func (ui *App) valueSection(st collection.Stats) fyne.CanvasObject {
	today := service.Today()
	sel := ui.chartRange()
	vc := service.BuildValueChart(ui.g.cards, st, ui.g.prices, sel, today)
	from, historyStart := vc.From, vc.HistoryStart

	series := []Series{
		{Name: "Market value", Color: colorMarket, Points: vc.MarketValue},
		{Name: "Price paid", Color: colorPaid, Points: vc.PricePaid, Step: true},
	}
	if vc.Estimate != nil { // values older purchases before recorded prices begin
		series = append(series, Series{
			Name: "At today's prices (estimate)", Color: colorEstimate, Points: vc.Estimate, Step: true,
		})
	}
	chart := NewLineChart(series, nil)
	chart.Empty = "Add cards to see value over time"
	if !from.IsZero() {
		chart.XMin, chart.XMax = from, today
	}

	picker := widget.NewRadioGroup(service.ChartRanges, nil)
	picker.Horizontal = true
	picker.Required = true
	picker.SetSelected(sel)
	picker.OnChanged = func(r string) {
		if r == "" || r == sel {
			return
		}
		if err := ui.settings.Update(func(s *settings.Settings) { s.ChartRange = r }); err != nil {
			slog.Warn("could not save chart range", "err", err)
		}
		ui.showHome()
	}

	// Price history status and, for games that offer it, the Load price history control.
	var notes []string
	var controls fyne.CanvasObject = container.NewHBox()
	historyLoaded := ui.g.prices.HistoryLoadedOn()
	src, canLoadHistory := ui.g.game.(game.PriceHistorySource)
	if canLoadHistory {
		label := "Load price history"
		if historyLoaded != "" {
			label = "Update price history"
		}
		gs := ui.g
		load := widget.NewButtonWithIcon(label, theme.DownloadIcon(), func() { ui.loadPriceHistory(gs) })
		if ui.g.loadingHistory {
			load.SetText("Loading price history…")
			load.Disable()
		}
		controls = container.NewHBox(load)
	}
	switch {
	case canLoadHistory && historyLoaded == "":
		notes = append(notes, "Market value uses daily prices recorded each day you open the app. "+
			"Load "+src.PriceHistoryInfo()+" to fill in the history.")
	case !historyStart.IsZero():
		notes = append(notes, fmt.Sprintf("Market value: daily market prices since %s.", historyStart.Format("Jan 2, 2006")))
	default:
		notes = append(notes, "Market value uses daily prices recorded each day you open the app.")
	}
	notes = append(notes, "Price paid: running total by acquired date.")
	if len(series) == 3 {
		notes = append(notes, "Before recorded prices begin, the grey line values the cards you owned at today's prices.")
	}

	var header fyne.CanvasObject = container.NewBorder(nil, nil, nil, picker, sectionTitle("Value Over Time"))
	if isMobile() { // not enough width for both on one line
		header = container.NewVBox(sectionTitle("Value Over Time"), picker)
	}
	return container.NewVBox(header, chart, caption(strings.Join(notes, " ")), controls)
}

// loadPriceHistory loads a game's daily price history from its history source.
func (ui *App) loadPriceHistory(gs *gameState) {
	if _, ok := gs.svc.PriceHistorySource(); !ok || gs.loadingHistory || len(gs.store.IDs()) == 0 {
		return
	}
	gs.loadingHistory = true
	ui.statusLabel.SetText("Loading price history…")
	ui.refreshCurrentPage()
	go func() {
		matched, err := gs.svc.LoadPriceHistory(func(msg string) {
			runOnUI(func() { ui.statusLabel.SetText(msg) })
		})
		runOnUI(func() {
			gs.loadingHistory = false
			if err != nil {
				slog.Warn("loading price history failed", "game", gs.game.ID(), "err", err)
				ui.statusLabel.SetText(fmt.Sprintf("Price history failed: %v", err))
			} else {
				ui.statusLabel.SetText(fmt.Sprintf("Loaded price history for %d printings", matched))
			}
			ui.refreshCurrentPage()
		})
	}()
}
