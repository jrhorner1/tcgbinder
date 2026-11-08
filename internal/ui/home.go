package ui

import (
	"fmt"
	"image/color"
	"log/slog"
	"sort"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"github.com/jrhorner1/mtgmanager/internal/collection"
)

var (
	colorGain = color.NRGBA{R: 0x3f, G: 0xb9, B: 0x50, A: 0xff}
	colorLoss = color.NRGBA{R: 0xe5, G: 0x53, B: 0x4b, A: 0xff}
)

// statTile is a dashboard card: small caption, large value, muted detail line.
func statTile(caption, value, detail string, valueColor color.Color) fyne.CanvasObject {
	bg := canvas.NewRectangle(theme.Color(theme.ColorNameInputBackground))
	bg.CornerRadius = 8
	if valueColor == nil {
		valueColor = theme.Color(theme.ColorNameForeground)
	}
	c := canvas.NewText(caption, theme.Color(theme.ColorNamePlaceHolder))
	c.TextSize = theme.CaptionTextSize()
	v := canvas.NewText(value, valueColor)
	v.TextSize = 22
	v.TextStyle = fyne.TextStyle{Bold: true}
	d := canvas.NewText(detail, theme.Color(theme.ColorNamePlaceHolder))
	d.TextSize = theme.CaptionTextSize()
	return container.NewStack(bg, container.New(layout.NewCustomPaddedLayout(10, 10, 14, 14), container.NewVBox(c, v, d)))
}

func sectionTitle(text string) fyne.CanvasObject {
	t := canvas.NewText(text, theme.Color(theme.ColorNameForeground))
	t.TextSize = theme.Size(theme.SizeNameSubHeadingText)
	t.TextStyle = fyne.TextStyle{Bold: true}
	return container.NewVBox(widget.NewSeparator(), container.NewPadded(t))
}

func pageTitle(text string) *canvas.Text {
	t := canvas.NewText(text, theme.Color(theme.ColorNameForeground))
	t.TextSize = theme.TextHeadingSize()
	t.TextStyle = fyne.TextStyle{Bold: true}
	return t
}

func caption(text string) *widget.Label {
	l := widget.NewLabel(text)
	l.Wrapping = fyne.TextWrapWord
	l.TextStyle = fyne.TextStyle{Italic: true}
	return l
}

func gainLoss(cost, market float64) (string, string, color.Color) {
	diff := market - cost
	col := color.Color(colorGain)
	sign := "+"
	if diff < 0 {
		col, sign = colorLoss, ""
	}
	pct := ""
	if cost > 0 {
		pct = fmt.Sprintf("%s%.1f%% vs. price paid", sign, diff/cost*100)
	}
	return sign + formatUSD(diff), pct, col
}

func finishLabel(c collection.Card) string {
	if c.Finish == "" || c.Finish == "nonfoil" {
		return ""
	}
	return " " + c.Finish
}

// topCardsTable lists the most valuable entries.
func topCardsTable(cards []collection.CardValue) fyne.CanvasObject {
	if len(cards) == 0 {
		return caption("No market prices yet. Use Refresh Prices to fetch them from Scryfall.")
	}
	rows := container.NewVBox()
	for i, cv := range cards {
		name := widget.NewLabel(fmt.Sprintf("%d. %s", i+1, cv.Card.Name))
		name.Truncation = fyne.TextTruncateEllipsis
		where := widget.NewLabel(setNumber(cv.Card.Set, cv.Card.Number) + finishLabel(cv.Card))
		qty := ""
		if cv.Card.Quantity > 1 {
			qty = fmt.Sprintf("%d × %s = ", cv.Card.Quantity, formatUSD(cv.Each))
		}
		value := widget.NewLabelWithStyle(qty+formatUSD(cv.Total), fyne.TextAlignTrailing, fyne.TextStyle{Bold: true})
		rows.Add(container.NewBorder(nil, nil, nil, container.NewHBox(where, value), name))
	}
	return rows
}

func completionItems(sets []collection.SetStats) []BarItem {
	items := make([]BarItem, 0, len(sets))
	for _, s := range sets {
		label := s.Code
		if s.Name != "" {
			label += " · " + s.Name
		}
		if c := s.Completion(); c >= 0 {
			items = append(items, BarItem{label, c, fmt.Sprintf("%d / %d  (%.0f%%)", s.UniqueNumbers, s.SetSize, c*100)})
		} else {
			items = append(items, BarItem{label, 0, fmt.Sprintf("%d owned · set size unknown", s.UniqueNumbers)})
		}
	}
	return items
}

// showHome renders the collection dashboard in the right pane.
func (ui *App) showHome() {
	ui.page = "home"
	st := collection.ComputeStats(ui.g.cards, ui.g.sets)

	refresh := widget.NewButtonWithIcon("Refresh Prices", theme.ViewRefreshIcon(), func() { ui.refreshPrices(ui.g) })
	if ui.g.refreshing {
		refresh.Disable()
		refresh.SetText("Refreshing…")
	}
	asOf := "never"
	if st.PricesAsOf != "" {
		asOf = st.PricesAsOf
	}
	header := container.NewBorder(nil, nil, nil, refresh, container.NewPadded(pageTitle("Collection Overview")))

	paidDetail := "all cards have a price paid"
	if st.CostUnknown > 0 {
		paidDetail = fmt.Sprintf("%d cards without a price paid", st.CostUnknown)
	}
	marketDetail := "prices as of " + asOf
	if st.MarketUnknown > 0 {
		marketDetail += fmt.Sprintf(" · %d unpriced", st.MarketUnknown)
	}
	diff, pct, col := gainLoss(st.Cost, st.Market)
	tiles := container.NewGridWithColumns(cols(4),
		statTile("CARDS", fmt.Sprintf("%d", st.Copies), fmt.Sprintf("%d entries · %d sets", st.Entries, st.Sets), nil),
		statTile("PRICE PAID", formatUSD(st.Cost), paidDetail, nil),
		statTile("MARKET VALUE", formatUSD(st.Market), marketDetail, colorMarket),
		statTile("GAIN / LOSS", diff, pct, col),
	)

	// Set completion.
	var completion fyne.CanvasObject = NewBarChart(completionItems(st.BySet), 1, colorMarket)
	if len(st.BySet) == 0 {
		completion = caption("No sets yet.")
	}
	completionNote := "Distinct collector numbers owned out of the set's total card count on Scryfall."
	if ui.g.sets == nil {
		completionNote = "Loading set sizes from Scryfall…"
	}

	// Value by set, top 10.
	bySet := append([]collection.SetStats(nil), st.BySet...)
	sort.Slice(bySet, func(i, j int) bool { return bySet[i].Market > bySet[j].Market })
	var valueItems []BarItem
	for _, s := range bySet {
		if len(valueItems) == 10 || s.Market <= 0 {
			break
		}
		valueItems = append(valueItems, BarItem{s.Code + " · " + s.Name, s.Market, formatUSD(s.Market)})
	}
	var valueBySet fyne.CanvasObject = NewBarChart(valueItems, 0, colorPaid)
	if len(valueItems) == 0 {
		valueBySet = caption("No market prices yet.")
	}

	content := container.NewVBox(
		header,
		tiles,
		ui.valueSection(st),
		sectionTitle("Set Completion"),
		caption(completionNote),
		completion,
		sectionTitle("Value by Set"),
		valueBySet,
		sectionTitle("Most Valuable Cards"),
		topCardsTable(st.TopCards),
	)
	ui.setRightPane(container.NewVScroll(container.NewPadded(content)))
}

// showSet renders one set's summary in the right pane.
func (ui *App) showSet(code string) {
	ui.page = "set:" + code
	var cards []collection.Card
	for _, c := range ui.g.cards {
		if strings.EqualFold(c.Set, code) {
			cards = append(cards, c)
		}
	}
	st := collection.ComputeStats(cards, ui.g.sets)
	if len(st.BySet) == 0 {
		ui.showHome()
		return
	}
	s := st.BySet[0]

	title := s.Name
	if title == "" {
		title = s.Code
	}
	sub := s.Code
	if s.Released != "" {
		sub += " · released " + s.Released
	}
	completion := "unknown"
	completionDetail := "set size not loaded"
	if c := s.Completion(); c >= 0 {
		completion = fmt.Sprintf("%.0f%%", c*100)
		completionDetail = fmt.Sprintf("%d of %d cards · %d missing", s.UniqueNumbers, s.SetSize, max(s.SetSize-s.UniqueNumbers, 0))
	}
	diff, pct, col := gainLoss(s.Cost, s.Market)

	content := container.NewVBox(
		container.NewPadded(container.NewVBox(pageTitle(title), caption(sub))),
		container.NewGridWithColumns(cols(4),
			statTile("COMPLETION", completion, completionDetail, colorMarket),
			statTile("COPIES", fmt.Sprintf("%d", s.Copies), fmt.Sprintf("%d entries", s.Entries), nil),
			statTile("MARKET VALUE", formatUSD(s.Market), "price paid "+formatUSD(s.Cost), colorMarket),
			statTile("GAIN / LOSS", diff, pct, col),
		),
	)
	if c := s.Completion(); c >= 0 {
		content.Add(NewBarChart([]BarItem{{"Owned", c, fmt.Sprintf("%.0f%%", c*100)}}, 1, colorMarket))
	}
	content.Add(sectionTitle("Most Valuable in This Set"))
	content.Add(topCardsTable(st.TopCards))
	ui.setRightPane(container.NewVScroll(container.NewPadded(content)))
}

func (ui *App) setRightPane(obj fyne.CanvasObject) {
	ui.rightPane.Objects = []fyne.CanvasObject{obj}
	ui.rightPane.Refresh()
}

// refreshCurrentPage re-renders a dashboard page after the data under it changed.
// The card editor is left alone so in-progress edits aren't lost.
func (ui *App) refreshCurrentPage() {
	switch {
	case ui.page == "home":
		ui.showHome()
	case strings.HasPrefix(ui.page, "set:"):
		ui.showSet(strings.TrimPrefix(ui.page, "set:"))
	}
}

// refreshPrices fetches a game's current prices from its catalog in the background.
func (ui *App) refreshPrices(gs *gameState) {
	if gs.refreshing {
		return
	}
	gs.refreshing = true
	ui.statusLabel.SetText(fmt.Sprintf("Refreshing %s prices…", gs.game.Name()))
	ui.refreshCurrentPage()
	go func() {
		n, err := gs.svc.RefreshPrices(func(done, total int) {
			runOnUI(func() { ui.statusLabel.SetText(fmt.Sprintf("Refreshing prices… %d of %d cards", done, total)) })
		})
		runOnUI(func() {
			gs.refreshing = false
			if err != nil {
				slog.Warn("price refresh failed", "game", gs.game.ID(), "err", err)
				ui.statusLabel.SetText(fmt.Sprintf("Price refresh failed: %v", err))
			} else {
				ui.statusLabel.SetText(fmt.Sprintf("Prices updated for %d entries", n))
			}
			ui.RefreshDataIndex()
		})
	}()
}

// StartBackgroundSync, for every game: loads set sizes, and once a day refreshes
// market prices so the value-over-time chart gains a point each day the app is used.
func (ui *App) StartBackgroundSync() {
	for _, gs := range ui.games {
		gs := gs
		go func() {
			sets, err := gs.svc.Sets()
			if err != nil {
				slog.Warn("could not load set list", "game", gs.game.ID(), "err", err)
				return
			}
			runOnUI(func() {
				gs.sets = sets
				ui.RefreshDataIndex()
			})
		}()
		// Desktop loads price history automatically the first time; on phones the large
		// download waits for the user to ask (Home → Load price history).
		if _, ok := gs.svc.CanBackfill(); ok && !isMobile() && gs.prices.BackfilledOn() == "" && len(gs.cards) > 0 {
			ui.backfillPrices(gs)
		}
		if gs.store.PricesStale() {
			ui.refreshPrices(gs) // records today's prices when done
			continue
		}
		// Prices are current: still record today's value so the history has a point
		// for every day the app is used.
		if err := gs.svc.RecordToday(); err != nil {
			slog.Warn("could not record today's value", "game", gs.game.ID(), "err", err)
		}
	}
	ui.refreshCurrentPage()
}
