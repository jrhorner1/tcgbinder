package ui

import (
	"fmt"
	"log/slog"
	"slices"
	"strconv"
	"strings"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"github.com/jrhorner1/tcgbinder/internal/collection"
	"github.com/jrhorner1/tcgbinder/internal/imagecache"
)

// resultDetail is the second line of a search result row, e.g. "M19 #201 · Mythic · 2018 · $41.88".
func resultDetail(c *collection.Card, finishes []string) string {
	parts := []string{setNumber(strings.ToUpper(c.Set), c.CollectorNumber)}
	if c.Rarity != "" {
		parts = append(parts, titleCase(c.Rarity))
	}
	if len(c.ReleasedAt) >= 4 {
		parts = append(parts, c.ReleasedAt[:4])
	}
	for i, f := range finishes { // first finish with a price; name it unless it's the default
		if p, ok := c.Market[f]; ok {
			price := formatUSD(p)
			if i > 0 {
				price += " " + f
			}
			parts = append(parts, price)
			break
		}
	}
	return strings.Join(parts, " · ")
}

// showAddDialog searches Scryfall, lets the user pick the exact printing from every
// result, and adds it to the collection.
func (ui *App) showAddDialog() { ui.showAddDialogFor("", "") }

// showAddDialogFor opens Add already searching for text in set (either may be empty).
func (ui *App) showAddDialogFor(prefillText, prefillSet string) {
	var (
		results               []collection.Card
		nextPage              string
		searchText, searchSet string
		selected              *collection.Card
		searchGen             int // bumped per search so late pages from an old search are dropped
		imageGen              int // bumped per selection so a slow image can't replace a newer one
	)

	// --- Search row ---
	nameEntry := widget.NewEntry()
	g := ui.g.game
	textHint, setHint := g.SearchHints()
	nameEntry.SetPlaceHolder(textHint)
	setEntry := widget.NewEntry()
	setEntry.SetPlaceHolder(setHint)
	searchButton := widget.NewButtonWithIcon("Search", theme.SearchIcon(), nil)
	setBox := container.NewGridWrap(fyne.NewSize(130, setEntry.MinSize().Height), setEntry)
	searchRow := container.NewBorder(nil, nil, nil, container.NewHBox(setBox, searchButton), nameEntry)

	// --- Results ---
	resultsStatus := widget.NewLabel("Search by name, set code, or both.")
	resultsStatus.Wrapping = fyne.TextWrapWord
	loadMore := widget.NewButton("Load more results", nil)
	loadMore.Hide()

	resultsList := widget.NewList(
		func() int { return len(results) },
		func() fyne.CanvasObject {
			name := widget.NewLabel("")
			name.TextStyle = fyne.TextStyle{Bold: true}
			name.Truncation = fyne.TextTruncateEllipsis
			detail := widget.NewLabel("")
			detail.Truncation = fyne.TextTruncateEllipsis
			return container.NewVBox(name, detail)
		},
		func(id widget.ListItemID, obj fyne.CanvasObject) {
			c := &results[id]
			box := obj.(*fyne.Container)
			box.Objects[0].(*widget.Label).SetText(c.Name)
			box.Objects[1].(*widget.Label).SetText(resultDetail(c, g.Finishes()))
		},
	)
	resultsPane := container.NewBorder(resultsStatus, loadMore, nil, nil, resultsList)

	// --- Preview ---
	cardImage := canvas.NewImageFromResource(nil)
	cardImage.FillMode = canvas.ImageFillContain
	cardImage.SetMinSize(fyne.NewSize(220, 306)) // card aspect ratio
	rulesLabel := widget.NewLabel("Select a result to preview it.")
	rulesLabel.Wrapping = fyne.TextWrapWord
	previewPane := container.NewBorder(cardImage, nil, nil, nil, container.NewVScroll(rulesLabel))

	body := container.NewHSplit(resultsPane, previewPane)
	body.Offset = 0.55
	if isMobile() {
		body = container.NewVSplit(resultsPane, previewPane)
		body.Offset = 0.5
	}

	// --- Ownership details ---
	qtyEntry := widget.NewEntry()
	qtyEntry.SetText("1")
	priceEntry := widget.NewEntry()
	priceEntry.SetPlaceHolder("0.00")
	defaultDate := ""
	if ui.settings.Get().DefaultDate == "today" {
		defaultDate = time.Now().Format("2006-01-02")
	}
	dateEntry := newDatePicker(defaultDate)
	finishSelect := newFinishPicker(ui.g.game)
	finishSelect.SetFinishes(nil, "")
	condSelect := widget.NewSelect([]string{"NM", "LP", "MP", "HP", "DMG"}, nil)
	condSelect.SetSelected(ui.settings.Get().DefaultCondition)

	detailCols := 2
	if isMobile() {
		detailCols = 1
	}
	details := container.NewGridWithColumns(detailCols,
		widget.NewForm(
			widget.NewFormItem("Quantity", qtyEntry),
			widget.NewFormItem("Price paid ($)", priceEntry),
			widget.NewFormItem("Acquired", dateEntry),
		),
		widget.NewForm(
			widget.NewFormItem("Finish", finishSelect.Select),
			widget.NewFormItem("Condition", condSelect),
		),
	)

	content := container.NewBorder(
		container.NewVBox(searchRow, widget.NewSeparator()),
		container.NewVBox(widget.NewSeparator(), details),
		nil, nil, body,
	)

	// --- Behavior ---
	updateStatus := func(total int) {
		switch {
		case len(results) == 0:
			resultsStatus.SetText("No cards found. Check the spelling or set code.")
		case total > len(results):
			resultsStatus.SetText(fmt.Sprintf("Showing %d of %d printings", len(results), total))
		case total == 1:
			resultsStatus.SetText("1 printing found")
		default:
			resultsStatus.SetText(fmt.Sprintf("%d printings found", total))
		}
		if nextPage != "" {
			loadMore.Show()
		} else {
			loadMore.Hide()
		}
	}

	fetchPage := func(gen int, page string) {
		loadMore.Disable()
		searchButton.Disable()
		go func() {
			res, err := g.Search(searchText, searchSet, page)
			runOnUI(func() {
				searchButton.Enable()
				loadMore.Enable()
				if gen != searchGen {
					return
				}
				if err != nil {
					slog.Warn("catalog search failed", "game", g.ID(), "err", err)
					resultsStatus.SetText(fmt.Sprintf("Search failed: %v", err))
					return
				}
				results = append(results, res.Cards...)
				nextPage = res.NextPage
				resultsList.Refresh()
				updateStatus(res.Total)
				if page == "" && len(results) > 0 {
					resultsList.Select(0)
				}
			})
		}()
	}

	runSearch := func() {
		text, set := strings.TrimSpace(nameEntry.Text), strings.TrimSpace(setEntry.Text)
		if text == "" && set == "" {
			resultsStatus.SetText("Enter a card name, a set code, or both.")
			return
		}
		searchGen++
		searchText, searchSet = text, set
		results, nextPage, selected = nil, "", nil
		resultsList.UnselectAll()
		resultsList.Refresh()
		loadMore.Hide()
		resultsStatus.SetText("Searching…")
		slog.Debug("catalog search", "game", g.ID(), "text", text, "set", set)
		fetchPage(searchGen, "")
	}
	searchButton.OnTapped = runSearch
	nameEntry.OnSubmitted = func(string) { runSearch() }
	setEntry.OnSubmitted = func(string) { runSearch() }
	loadMore.OnTapped = func() {
		if nextPage != "" {
			fetchPage(searchGen, nextPage)
		}
	}

	var addButton *widget.Button
	resultsList.OnSelected = func(id widget.ListItemID) {
		card := results[id]
		selected = &card
		addButton.Enable()

		// Offer only this printing's finishes, keeping the user's choice when it has it.
		prev := finishSelect.Finish()
		if !slices.Contains(card.Finishes, prev) {
			prev = ""
		}
		finishSelect.SetFinishes(card.Finishes, prev)

		rulesLabel.SetText(fmt.Sprintf("%s\n%s — %s\n%s\n\n%s",
			card.Name, card.SetName, setNumber(strings.ToUpper(card.Set), card.CollectorNumber), card.TypeLine, card.RulesText))

		imageGen++
		gen := imageGen
		if !ui.settings.Get().ShowImages {
			cardImage.Resource = theme.FileImageIcon()
			cardImage.Refresh()
			return
		}
		cardImage.Resource = theme.DownloadIcon()
		cardImage.Refresh()
		go func() {
			img, err := imagecache.Fetch(card.ImageURI)
			runOnUI(func() {
				if gen != imageGen {
					return
				}
				if err != nil || len(img) == 0 {
					cardImage.Resource = theme.BrokenImageIcon()
				} else {
					cardImage.Resource = fyne.NewStaticResource(card.ID+".jpg", img)
				}
				cardImage.Refresh()
			})
		}()
	}

	d := dialog.NewCustomWithoutButtons("Add Card", content, ui.window)
	cancelButton := widget.NewButton("Cancel", func() { d.Hide() })
	addButton = widget.NewButtonWithIcon("Add to Collection", theme.ContentAddIcon(), func() {
		if selected == nil {
			return
		}
		qty, err := strconv.Atoi(strings.TrimSpace(qtyEntry.Text))
		if err != nil || qty <= 0 {
			dialog.ShowError(fmt.Errorf("quantity must be a whole number of at least 1"), ui.window)
			return
		}
		price := 0.0
		if p := strings.TrimSpace(priceEntry.Text); p != "" {
			if price, err = strconv.ParseFloat(strings.TrimPrefix(p, "$"), 64); err != nil || price < 0 {
				dialog.ShowError(fmt.Errorf("price paid must be a number, e.g. 2.50"), ui.window)
				return
			}
		}

		newCard := *selected // catalog data from the search result
		newCard.AddPurchase(finishSelect.Finish(), collection.Condition(condSelect.Selected),
			collection.Purchase{Quantity: qty, PricePaid: price, AcquiredDate: pickedDate(dateEntry)})
		// Adding a printing you already own adds these copies to its card.
		_, merged, err := ui.g.store.AddCards([]collection.Card{newCard})
		if err != nil {
			dialog.ShowError(fmt.Errorf("save failed: %w", err), ui.window)
			return
		}
		d.Hide()
		ui.RefreshDataIndex()
		verb := "Added"
		if merged > 0 {
			verb = "Added to the copies you own:"
		}
		ui.statusLabel.SetText(fmt.Sprintf("%s %d× %s (%s)", verb, qty, selected.Name, setNumber(strings.ToUpper(selected.Set), selected.CollectorNumber)))
	})
	addButton.Importance = widget.HighImportance
	addButton.Disable()
	d.SetButtons([]fyne.CanvasObject{cancelButton, addButton})

	size := fyne.NewSize(960, 700)
	if isMobile() {
		size = ui.window.Canvas().Size() // full screen on phones
	}
	d.Resize(size)
	d.Show()
	ui.window.Canvas().Focus(nameEntry)
	if prefillText != "" || prefillSet != "" {
		nameEntry.SetText(prefillText)
		setEntry.SetText(prefillSet)
		runSearch()
	}
}
