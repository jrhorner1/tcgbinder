package ui

import (
	"fmt"
	"image/color"
	"log/slog"
	"sort"
	"strings"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"github.com/jrhorner1/tcgbinder/internal/collection"
	"github.com/jrhorner1/tcgbinder/internal/service"
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
	// The detail line shortens with "…" instead of forcing the tile wider (phones).
	d := widget.NewRichText(&widget.TextSegment{Text: detail, Style: widget.RichTextStyle{
		SizeName: theme.SizeNameCaptionText, ColorName: theme.ColorNamePlaceHolder}})
	d.Truncation = fyne.TextTruncateEllipsis
	return container.NewStack(bg, container.New(layout.NewCustomPaddedLayout(10, 10, 14, 14),
		container.NewVBox(c, v, container.New(layout.NewCustomPaddedLayout(-theme.Padding(), -theme.Padding(), -theme.Padding(), 0), d))))
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

func gainLoss(paid, value float64) (string, string, color.Color) {
	diff := value - paid
	col := color.Color(colorGain)
	sign := "+"
	if diff < 0 {
		col, sign = colorLoss, ""
	}
	pct := ""
	if paid > 0 {
		pct = fmt.Sprintf("%s%.1f%% vs. price paid", sign, diff/paid*100)
	}
	return sign + formatUSD(diff), pct, col
}

// topCardsTable lists valuable cards; tapping a row opens the card. With oneCopy
// the value shown is a single copy's (as ranked), otherwise the card's total.
func (ui *App) topCardsTable(cards []collection.CardValue, oneCopy bool) fyne.CanvasObject {
	if len(cards) == 0 {
		return caption("No market prices yet; they refresh automatically each day.")
	}
	g := ui.g.game
	rows := container.NewVBox()
	for i, cv := range cards {
		c := cv.Card
		name := widget.NewLabel(fmt.Sprintf("%d. %s", i+1, c.Name))
		name.Truncation = fyne.TextTruncateEllipsis
		where := setNumber(c.Set, c.CollectorNumber) + " · " + copiesSummary(c, g)
		amount := formatUSD(cv.Total) // every copy
		if oneCopy {
			amount = formatUSD(cv.Each) // one copy of the most valuable finish owned
		}
		value := widget.NewLabelWithStyle(amount, fyne.TextAlignTrailing, fyne.TextStyle{Bold: true})
		rows.Add(newTapRow(container.NewBorder(nil, nil, nil, container.NewHBox(widget.NewLabel(where), value), name),
			func() { ui.selectCardNode(c) }))
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
			items = append(items, BarItem{label, c, fmt.Sprintf("%d / %d  (%.0f%%)", s.Collected, s.SetSize, c*100)})
		} else {
			items = append(items, BarItem{label, 0, fmt.Sprintf("%d owned · set size unknown", s.Collected)})
		}
	}
	return items
}

// showHome renders the collection dashboard in the right pane.
func (ui *App) showHome() {
	ui.page = "home"
	st := collection.ComputeStats(ui.g.cards, ui.g.sets)

	asOf := "never"
	if st.PricesAsOf != "" {
		asOf = st.PricesAsOf
	}
	header := container.NewPadded(pageTitle(ui.g.game.Name()))

	paidDetail := "all cards have a price paid"
	if st.PricePaidUnknown > 0 {
		paidDetail = fmt.Sprintf("%d cards without a price paid", st.PricePaidUnknown)
	}
	marketDetail := "prices as of " + asOf
	if st.Unpriced > 0 {
		marketDetail += fmt.Sprintf(" · %d unpriced", st.Unpriced)
	}
	diff, pct, col := gainLoss(st.PricePaid, st.MarketValue)
	tiles := container.NewGridWithColumns(cols(4),
		statTile("CARDS", fmt.Sprintf("%d", st.Cards), fmt.Sprintf("%s · %s",
			plural(collection.UniqueCards(ui.g.cards), "unique card", "unique cards"), plural(st.Sets, "set", "sets")), nil),
		statTile("PRICE PAID", formatUSD(st.PricePaid), paidDetail, nil),
		statTile("MARKET VALUE", formatUSD(st.MarketValue), marketDetail, colorMarket),
		statTile("GAIN / LOSS", diff, pct, col),
	)

	// Set completion.
	bars := NewBarChart(completionItems(st.BySet), 1, colorMarket)
	bars.OnTapped = func(i int) { ui.openSet(st.BySet[i].Code) } // the set's page
	var completion fyne.CanvasObject = bars
	if len(st.BySet) == 0 {
		completion = caption("No sets yet.")
	}
	completionNote := "Distinct collector numbers owned out of the set's total card count on Scryfall. Tap a set to open its page."
	if ui.g.sets == nil {
		completionNote = "Loading set sizes from Scryfall…"
	}

	// Value by set, top 10.
	bySet := append([]collection.SetStats(nil), st.BySet...)
	sort.Slice(bySet, func(i, j int) bool { return bySet[i].MarketValue > bySet[j].MarketValue })
	var valueItems []BarItem
	for _, s := range bySet {
		if len(valueItems) == 10 || s.MarketValue <= 0 {
			break
		}
		valueItems = append(valueItems, BarItem{s.Code + " · " + s.Name, s.MarketValue, formatUSD(s.MarketValue)})
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
		ui.topCardsTable(st.TopCards, false),
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
		completionDetail = fmt.Sprintf("%d of %d cards · %d missing", s.Collected, s.SetSize, max(s.SetSize-s.Collected, 0))
	}
	diff, pct, col := gainLoss(s.PricePaid, s.MarketValue)

	content := container.NewVBox(
		container.NewPadded(container.NewVBox(pageTitle(title), caption(sub))),
		container.NewGridWithColumns(cols(4),
			statTile("COMPLETION", completion, completionDetail, colorMarket),
			statTile("CARDS", fmt.Sprintf("%d", s.Cards), plural(s.UniqueCards, "unique card", "unique cards"), nil),
			statTile("MARKET VALUE", formatUSD(s.MarketValue), "price paid "+formatUSD(s.PricePaid), colorMarket),
			statTile("GAIN / LOSS", diff, pct, col),
		),
	)
	if c := s.Completion(); c >= 0 {
		content.Add(NewBarChart([]BarItem{{"Owned", c, fmt.Sprintf("%.0f%%", c*100)}}, 1, colorMarket))
	}
	content.Add(sectionTitle("Most Valuable in This Set"))
	content.Add(caption("Ranked by the value of one copy."))
	content.Add(ui.topCardsTable(collection.TopByEach(cards, 10), true))
	content.Add(sectionTitle("Missing Cards"))
	content.Add(ui.missingSection(s))
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
	case ui.page == "tools":
		ui.showTools()
	case ui.page == "settings":
		ui.showSettings()
	case strings.HasPrefix(ui.page, "search:"):
		ui.showSearchResults(strings.TrimPrefix(ui.page, "search:"))
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
				ui.statusLabel.SetText(fmt.Sprintf("Prices updated for %d cards", n))
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
		if !isMobile() && gs.svc.AutoLoadHistoryDue() {
			ui.loadPriceHistory(gs)
		}
		if gs.svc.PricesDue() {
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
	// Prices refresh on a schedule rather than by button: check hourly and refresh
	// once the stored prices are from an earlier day (catalogs update about daily).
	go func() {
		for range time.Tick(service.PriceCheckInterval) {
			runOnUI(func() {
				for _, gs := range ui.games {
					if gs.svc.PricesDue() {
						ui.refreshPrices(gs)
					}
				}
			})
		}
	}()
}

// missingSection offers the set's cards you don't own. The full card list comes from
// the game's catalog, so it loads only when asked.
func (ui *App) missingSection(s collection.SetStats) fyne.CanvasObject {
	box := container.NewVBox()
	gs := ui.g
	code := s.Code
	label := "Show missing cards"
	if s.SetSize > 0 {
		label = fmt.Sprintf("Show the %d missing cards", max(s.SetSize-s.Collected, 0))
	}
	var show *widget.Button
	show = widget.NewButtonWithIcon(label, theme.SearchIcon(), func() {
		show.Disable()
		show.SetText("Loading the set's cards…")
		go func() {
			setCards, err := gs.game.SetCards(code)
			runOnUI(func() {
				if err != nil {
					show.Enable()
					show.SetText(label)
					ui.statusLabel.SetText(fmt.Sprintf("Could not load %s: %v", code, err))
					return
				}
				var owned []collection.Card
				for _, c := range gs.cards {
					if strings.EqualFold(c.Set, code) {
						owned = append(owned, c)
					}
				}
				missing := collection.Missing(setCards, owned)
				box.Objects = nil
				if len(missing) == 0 {
					box.Add(caption("You have every card in this set."))
				} else {
					box.Add(caption(fmt.Sprintf("%s you don't own yet. Tap one to add it.", plural(len(missing), "card", "cards"))))
				}
				for _, c := range missing {
					c := c
					price := ""
					if p, ok := c.Market[gs.game.Finishes()[0]]; ok {
						price = formatUSD(p)
					}
					num := widget.NewLabel(c.CollectorNumber)
					num.Alignment = fyne.TextAlignTrailing
					name := widget.NewLabel(c.Name)
					name.Truncation = fyne.TextTruncateEllipsis
					right := widget.NewLabel(strings.TrimSpace(titleCase(c.Rarity) + "  " + price))
					row := container.NewBorder(nil, nil, container.NewGridWrap(fyne.NewSize(56, num.MinSize().Height), num), right, name)
					box.Add(newTapRow(row, func() { ui.showAddDialogFor(c.Name, code) }))
				}
				box.Refresh()
			})
		}()
	})
	box.Add(show)
	return box
}
