package ui

import (
	"encoding/json"
	"fmt"
	"image/color"
	"log/slog"
	"slices"
	"sort"
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
	"github.com/jrhorner1/tcgbinder/internal/game"
	"github.com/jrhorner1/tcgbinder/internal/imagecache"
)

var conditionLabels = []struct {
	Code  collection.Condition
	Label string
}{
	{collection.ConditionNearMint, "NM — Near Mint"},
	{collection.ConditionLightPlay, "LP — Lightly Played"},
	{collection.ConditionModerate, "MP — Moderately Played"},
	{collection.ConditionHeavyPlay, "HP — Heavily Played"},
	{collection.ConditionDamaged, "DMG — Damaged"},
}

func conditionFromLabel(label string) collection.Condition {
	for _, l := range conditionLabels {
		if l.Label == label {
			return l.Code
		}
	}
	return collection.ConditionNearMint
}

func titleCase(s string) string {
	if s == "" {
		return s
	}
	return strings.ToUpper(s[:1]) + s[1:]
}

// marketPricesText lists the stored price for every finish, e.g. "$0.23 nonfoil · $0.20 foil".
func marketPricesText(c collection.Card, g game.Game) string {
	finishes := g.Finishes()
	if len(c.Market) == 0 {
		return "not available yet"
	}
	var parts []string
	seen := map[string]bool{}
	for _, f := range append(append([]string{}, finishes...), c.Finishes...) {
		if p, ok := c.Market[f]; ok && !seen[f] {
			seen[f] = true
			parts = append(parts, formatUSD(p)+" "+g.FinishName(f))
		}
	}
	out := strings.Join(parts, " · ")
	if c.PricesUpdated != "" {
		out += "  (as of " + c.PricesUpdated + ")"
	}
	return out
}

// infoRow is a read-only "Label   value" line for the card details section.
func infoRow(label, value string) *widget.FormItem {
	v := widget.NewLabel(value)
	v.Wrapping = fyne.TextWrapWord
	return widget.NewFormItem(label, v)
}

// selectCardNode shows a card of the active game in the tree and opens it in the editor.
func (ui *App) selectCardNode(c collection.Card) { ui.selectCard(collection.CardKey(c), c.Set) }

// selectCard is selectCardNode by card key.
func (ui *App) selectCard(key, set string) {
	gameID := ui.g.game.ID()
	id := prefixCard + gameID + ":" + key
	ui.tree.OpenBranch(gameNode(gameID))
	ui.tree.UnselectAll() // so selecting the same node again still fires OnSelected
	ui.tree.Select(id)
	ui.tree.ScrollTo(id)
}

// renderEditor shows one collection card: its image, value, editable ownership
// details, card information and other copies of the same card.
func (ui *App) renderEditor(card collection.Card) {
	ui.page = "card"
	ui.editorGen++
	gen := ui.editorGen
	oldKey := collection.CardKey(card)

	// --- Image (loaded in the background) ---
	img := canvas.NewImageFromResource(theme.DownloadIcon())
	img.FillMode = canvas.ImageFillContain
	img.SetMinSize(fyne.NewSize(250, 349)) // card aspect ratio
	switch {
	case !ui.settings.Get().ShowImages:
		img.Resource = theme.FileImageIcon()
		img.SetMinSize(fyne.NewSize(120, 120)) // a small placeholder: images are turned off
	case card.ImageURI == "":
		img.Resource = theme.BrokenImageIcon()
	default:
		go func() {
			b, err := imagecache.Fetch(card.ImageURI)
			runOnUI(func() {
				if gen != ui.editorGen {
					return
				}
				if err != nil || len(b) == 0 {
					slog.Warn("image load failed", "card", card.Name, "err", err)
					img.Resource = theme.BrokenImageIcon()
				} else {
					img.Resource = fyne.NewStaticResource(card.ID+".jpg", b)
				}
				img.Refresh()
			})
		}()
	}
	imageColumn := container.NewVBox(img)
	if label, link := ui.g.game.CardLink(card); link != nil {
		imageColumn.Add(container.NewCenter(widget.NewHyperlink(label, link)))
	}

	// --- Header ---
	subParts := []string{}
	if card.SetName != "" {
		subParts = append(subParts, card.SetName)
	}
	subParts = append(subParts, setNumber(card.Set, card.CollectorNumber))
	if card.Rarity != "" {
		subParts = append(subParts, titleCase(card.Rarity))
	}
	header := container.NewVBox(pageTitle(card.Name), caption(strings.Join(subParts, " · ")))

	// --- Value tiles (saved state) ---
	g := ui.g.game
	qty := card.CopyCount()
	value, unpriced := card.MarketValue()
	paid, unknownPaid := card.PricePaid()
	marketValue, marketDetail := "—", "no market price yet"
	if unpriced < qty {
		marketValue = formatUSD(value)
		marketDetail = plural(qty, "copy", "copies")
		if unpriced > 0 {
			marketDetail += fmt.Sprintf(" · %d unpriced", unpriced)
		}
	}
	paidValue, paidDetail := "—", "price paid not recorded"
	if unknownPaid < qty {
		paidValue = formatUSD(paid)
		paidDetail = "for " + plural(qty-unknownPaid, "copy", "copies")
	}
	gainValue, gainDetail, gainColor := "—", "needs both prices", theme.Color(theme.ColorNameForeground)
	if unpriced == 0 && unknownPaid == 0 && qty > 0 {
		gainValue, gainDetail, gainColor = gainLoss(paid, value)
	}
	tiles := container.NewGridWithColumns(cols(3),
		statTile("MARKET VALUE", marketValue, marketDetail, colorMarket),
		statTile("PRICE PAID", paidValue, paidDetail, nil),
		statTile("GAIN / LOSS", gainValue, gainDetail, gainColor),
	)

	// --- Your copies: one row per purchase (finish, condition, quantity, price, date) ---
	condOptions := make([]string, len(collection.Conditions)) // short codes fit the table
	for i, c := range collection.Conditions {
		condOptions[i] = string(c)
	}
	type purchaseRow struct {
		finish *finishPicker
		cond   *widget.Select
		qty    *widget.Entry
		price  *widget.Entry
		date   *widget.DateEntry
		view   fyne.CanvasObject
	}
	var rows []*purchaseRow
	rowsBox := container.NewVBox()
	subtotals := widget.NewLabel("")
	subtotals.Wrapping = fyne.TextWrapWord
	errorLabel := widget.NewLabel("")
	errorLabel.Importance = widget.DangerImportance
	errorLabel.Wrapping = fyne.TextWrapWord
	errorLabel.Hide()
	saveButton := widget.NewButtonWithIcon("Save Changes", theme.DocumentSaveIcon(), nil)
	saveButton.Importance = widget.HighImportance
	revertButton := widget.NewButtonWithIcon("Revert", theme.ContentUndoIcon(), nil)
	deleteButton := widget.NewButtonWithIcon("Delete", theme.DeleteIcon(), nil)
	deleteButton.Importance = widget.DangerImportance

	// read builds the edited card from the rows; problem explains invalid input.
	read := func() (edited collection.Card, problem string) {
		edited = card
		edited.Copies = nil
		for _, r := range rows {
			n, err := strconv.Atoi(strings.TrimSpace(r.qty.Text))
			if err != nil || n < 0 {
				return edited, "Quantities must be whole numbers (0 or more)."
			}
			price := 0.0
			if p := strings.TrimPrefix(strings.TrimSpace(r.price.Text), "$"); p != "" {
				if price, err = strconv.ParseFloat(p, 64); err != nil || price < 0 {
					return edited, "Price paid must be a number, e.g. 2.50."
				}
			}
			if strings.TrimSpace(r.date.Text) != "" && r.date.Date == nil {
				return edited, "An acquired date isn't valid; pick one from the calendar."
			}
			edited.AddPurchase(r.finish.Finish(), collection.ParseCondition(r.cond.Selected),
				collection.Purchase{Quantity: n, PricePaid: price, AcquiredDate: pickedDate(r.date)})
		}
		return edited, ""
	}
	fingerprint := func(c collection.Card) string {
		b, _ := json.Marshal(c.PurchaseRows(g.Finishes()...))
		return string(b)
	}
	original := fingerprint(card)

	// update refreshes the subtotals and enables Save/Revert while anything changed.
	update := func() {
		edited, problem := read()
		if problem == "" {
			var parts []string
			for _, f := range edited.OwnedFinishes(g.Finishes()...) {
				var byCond []string
				for _, c := range collection.Conditions {
					if n := edited.ConditionCopyCount(f, c); n > 0 {
						byCond = append(byCond, fmt.Sprintf("%s %d", c, n))
					}
				}
				parts = append(parts, fmt.Sprintf("%s %d (%s)", g.FinishName(f), edited.FinishCopyCount(f), strings.Join(byCond, ", ")))
			}
			if len(parts) == 0 {
				subtotals.SetText("No copies — saving removes this card from your collection.")
			} else {
				subtotals.SetText(fmt.Sprintf("%s: %s", plural(edited.CopyCount(), "copy", "copies"), strings.Join(parts, " · ")))
			}
		}
		if problem != "" || fingerprint(edited) != original {
			saveButton.Enable()
			revertButton.Enable()
		} else {
			saveButton.Disable()
			revertButton.Disable()
		}
	}

	var addRow func(h collection.PurchaseRow)
	addRow = func(h collection.PurchaseRow) {
		r := &purchaseRow{finish: newFinishPicker(g), cond: widget.NewSelect(condOptions, nil),
			qty: widget.NewEntry(), price: widget.NewEntry(), date: newDatePicker(h.AcquiredDate)}
		r.finish.SetFinishes(card.Finishes, h.Finish) // only finishes this printing comes in
		r.cond.SetSelected(string(h.Condition))
		r.qty.SetText(strconv.Itoa(h.Quantity))
		r.price.SetPlaceHolder("price paid")
		if h.PricePaid > 0 {
			r.price.SetText(strconv.FormatFloat(h.PricePaid, 'f', 2, 64))
		}
		r.finish.OnChanged = func(string) { update() }
		r.cond.OnChanged = func(string) { update() }
		r.qty.OnChanged = func(string) { update() }
		r.price.OnChanged = func(string) { update() }
		r.date.OnChanged = func(*time.Time) { update() }
		remove := widget.NewButtonWithIcon("", theme.ContentRemoveIcon(), nil)
		var fields *fyne.Container
		if isMobile() {
			fields = container.NewGridWithColumns(2, r.finish.Select, r.cond, r.qty, r.price, r.date)
		} else {
			fields = container.New(purchaseColumns, r.finish.Select, r.cond, r.qty, r.price, r.date)
		}
		r.view = container.NewBorder(nil, nil, nil, remove, fields)
		remove.OnTapped = func() {
			rows = slices.DeleteFunc(rows, func(x *purchaseRow) bool { return x == r })
			rowsBox.Remove(r.view)
			update()
		}
		rows = append(rows, r)
		rowsBox.Add(r.view)
	}
	for _, h := range card.PurchaseRows(g.Finishes()...) {
		addRow(h)
	}
	firstFinish := g.Finishes()[0]
	if len(card.Finishes) > 0 {
		firstFinish = card.Finishes[0]
	}
	addPurchase := widget.NewButtonWithIcon("Add purchase", theme.ContentAddIcon(), func() {
		addRow(collection.PurchaseRow{Finish: firstFinish, Condition: collection.ConditionNearMint,
			Purchase: collection.Purchase{Quantity: 1, AcquiredDate: time.Now().Format("2006-01-02")}})
		update()
	})
	copiesSection := container.NewVBox()
	if !isMobile() { // phones show each purchase as a small grid instead of a table
		heads := container.New(purchaseColumns)
		for _, t := range []string{"Finish", "Condition", "Qty", "Paid each", "Acquired"} {
			l := widget.NewLabel(t)
			l.TextStyle = fyne.TextStyle{Bold: true}
			heads.Add(l)
		}
		spacer := canvas.NewRectangle(color.Transparent) // as wide as a row's remove button, so the heads line up
		spacer.SetMinSize(widget.NewButtonWithIcon("", theme.ContentRemoveIcon(), nil).MinSize())
		copiesSection.Add(container.NewBorder(nil, nil, nil, spacer, heads))
	}
	copiesSection.Add(rowsBox)
	copiesSection.Add(container.NewHBox(addPurchase))
	copiesSection.Add(subtotals)
	update()

	deleteCard := func() {
		ui.confirm("Delete from collection?",
			fmt.Sprintf("Remove all %s of %s (%s) from your collection?",
				plural(qty, "copy", "copies"), card.Name, setNumber(card.Set, card.CollectorNumber)),
			func() {
				if err := ui.g.store.RemoveCard(oldKey); err != nil {
					dialog.ShowError(fmt.Errorf("delete failed: %w", err), ui.window)
					return
				}
				ui.statusLabel.SetText(fmt.Sprintf("Deleted %s (%s)", card.Name, setNumber(card.Set, card.CollectorNumber)))
				ui.RefreshDataIndex()
				ui.tree.UnselectAll()
				ui.tree.Select(gameNode(ui.g.game.ID()))
			})
	}
	deleteButton.OnTapped = deleteCard

	save := func() {
		edited, problem := read()
		if problem != "" {
			errorLabel.SetText(problem)
			errorLabel.Show()
			return
		}
		errorLabel.Hide()
		if edited.CopyCount() == 0 {
			deleteCard()
			return
		}
		newKey, _, err := ui.g.store.SaveCard(oldKey, edited)
		if err != nil {
			dialog.ShowError(fmt.Errorf("save failed: %w", err), ui.window)
			return
		}
		ui.statusLabel.SetText(fmt.Sprintf("Saved %s (%s)", card.Name, plural(edited.CopyCount(), "copy", "copies")))
		ui.RefreshDataIndex()
		ui.selectCard(newKey, edited.Set)
	}
	saveButton.OnTapped = save
	revertButton.OnTapped = func() { ui.renderEditor(card) }

	buttons := container.NewBorder(nil, nil, deleteButton, container.NewHBox(revertButton, saveButton))
	changePrinting := widget.NewButtonWithIcon("Change printing…", theme.ViewRefreshIcon(), func() { ui.showChangePrinting(card) })
	rawData := widget.NewButtonWithIcon("Raw data", theme.DocumentIcon(), func() { ui.showRawData(card) })
	moreButtons := container.NewHBox(changePrinting, rawData)

	// --- Card details ---
	typeLabel := widget.NewLabel(card.TypeLine)
	typeLabel.TextStyle = fyne.TextStyle{Bold: true}
	typeLabel.Wrapping = fyne.TextWrapWord
	textLabel := widget.NewLabel(card.RulesText)
	textLabel.Wrapping = fyne.TextWrapWord

	detailItems := []*widget.FormItem{
		infoRow("Set", fmt.Sprintf("%s (%s)", card.SetName, card.Set)),
		infoRow("Collector number", card.CollectorNumber),
		infoRow("Rarity", titleCase(card.Rarity)),
	}
	for _, r := range ui.g.game.Details(card) { // game-specific rows, e.g. Magic's mana cost
		detailItems = append(detailItems, infoRow(r.Label, r.Value))
	}
	if card.Language != "" && card.Language != "en" {
		detailItems = append(detailItems, infoRow("Language", strings.ToUpper(card.Language)))
	}
	if card.ReleasedAt != "" {
		detailItems = append(detailItems, infoRow("Released", card.ReleasedAt))
	}
	detailItems = append(detailItems,
		infoRow("Printed in", finishNames(card.Finishes, g)),
		infoRow("Market prices", marketPricesText(card, ui.g.game)),
	)

	right := container.NewVBox(
		tiles,
		sectionTitle("Your Copies"),
		copiesSection,
		errorLabel,
		buttons,
		moreButtons,
		sectionTitle("Card"),
		typeLabel,
		textLabel,
		widget.NewForm(detailItems...),
	)

	// --- Other printings of the same card you own ---
	var others []collection.Card
	for _, c := range ui.g.cards {
		if collection.CardKey(c) == oldKey {
			continue
		}
		if c.Name == card.Name { // other printings of the same card
			others = append(others, c)
		}
	}
	if len(others) > 0 {
		sort.Slice(others, func(i, j int) bool {
			if others[i].ReleasedAt != others[j].ReleasedAt {
				return others[i].ReleasedAt > others[j].ReleasedAt
			}
			return collection.DefaultOrder(others[i], others[j]) < 0
		})
		right.Add(sectionTitle("Other Printings You Own"))
		for _, o := range others {
			o := o
			parts := []string{setNumber(o.Set, o.CollectorNumber)}
			if o.SetName != "" {
				parts[0] = o.SetName + " · " + parts[0]
			}
			parts = append(parts, copiesSummary(o, ui.g.game))
			btn := widget.NewButton(strings.Join(parts, " · "), func() { ui.selectCardNode(o) })
			btn.Alignment = widget.ButtonAlignLeading
			btn.Importance = widget.LowImportance
			right.Add(btn)
		}
	}

	body := container.NewBorder(nil, nil, container.NewPadded(imageColumn), nil, right)
	if isMobile() { // image above the details on narrow screens
		body = container.NewVBox(container.NewCenter(imageColumn), right)
	}
	ui.setRightPane(container.NewVScroll(container.NewPadded(container.NewVBox(header, body))))
}

// showRawData shows the card exactly as stored, as JSON.
func (ui *App) showRawData(card collection.Card) {
	b, err := json.MarshalIndent(card, "", "  ")
	if err != nil {
		dialog.ShowError(err, ui.window)
		return
	}
	text := widget.NewLabelWithStyle(string(b), fyne.TextAlignLeading, fyne.TextStyle{Monospace: true})
	scroll := container.NewScroll(text)
	size := fyne.NewSize(640, 480)
	if isMobile() {
		size = ui.window.Canvas().Size()
	}
	scroll.SetMinSize(fyne.NewSize(min(size.Width-40, 600), min(size.Height-160, 440)))
	copyButton := widget.NewButtonWithIcon("Copy", theme.ContentCopyIcon(), func() {
		fyne.CurrentApp().Clipboard().SetContent(string(b))
		ui.statusLabel.SetText("Copied " + card.Name + " as JSON")
	})
	d := dialog.NewCustom(card.Name+" — raw data", "Close", container.NewBorder(nil, container.NewHBox(copyButton), nil, nil, scroll), ui.window)
	d.Resize(size)
	d.Show()
}

// showChangePrinting moves the card to another printing of the same card, keeping
// quantity, condition, price paid and acquired date.
func (ui *App) showChangePrinting(card collection.Card) {
	gs := ui.g
	oldKey := collection.CardKey(card)
	status := widget.NewLabel("Finding every printing of " + card.Name + "…")
	var printings []collection.Card
	list := widget.NewList(
		func() int { return len(printings) },
		func() fyne.CanvasObject {
			name := widget.NewLabel("")
			name.TextStyle = fyne.TextStyle{Bold: true}
			name.Truncation = fyne.TextTruncateEllipsis
			detail := widget.NewLabel("")
			detail.Truncation = fyne.TextTruncateEllipsis
			return container.NewVBox(name, detail)
		},
		func(id widget.ListItemID, obj fyne.CanvasObject) {
			p := printings[id]
			box := obj.(*fyne.Container)
			title := p.SetName
			if p.ID == card.ID {
				title += "  (current)"
			}
			box.Objects[0].(*widget.Label).SetText(title)
			box.Objects[1].(*widget.Label).SetText(resultDetail(&p, gs.game.Finishes()))
		},
	)
	size := fyne.NewSize(560, 560)
	if isMobile() {
		size = ui.window.Canvas().Size()
	}
	var d dialog.Dialog
	list.OnSelected = func(id widget.ListItemID) {
		p := printings[id]
		if p.ID == card.ID {
			list.UnselectAll()
			return
		}
		moved := collection.WithPrinting(card, p)
		newKey, merged, err := gs.store.SaveCard(oldKey, moved)
		if err != nil {
			dialog.ShowError(fmt.Errorf("could not change printing: %w", err), ui.window)
			return
		}
		d.Hide()
		msg := fmt.Sprintf("Moved %s to %s", card.Name, setNumber(moved.Set, moved.CollectorNumber))
		for _, f := range card.OwnedFinishes() {
			if len(moved.Finishes) > 0 && !slices.Contains(moved.Finishes, f) {
				msg += fmt.Sprintf(" (%s copies became %s: the new printing doesn't come in %s)",
					gs.game.FinishName(f), gs.game.FinishName(moved.Finishes[0]), gs.game.FinishName(f))
			}
		}
		if merged {
			msg += ", merged with the copies you already had"
		}
		ui.statusLabel.SetText(msg)
		ui.RefreshDataIndex()
		ui.selectCard(newKey, moved.Set)
	}
	d = dialog.NewCustom("Change printing of "+card.Name, "Cancel", container.NewBorder(status, nil, nil, nil, list), ui.window)
	d.Resize(size)
	d.Show()
	go func() {
		found, err := gs.game.Printings(card)
		runOnUI(func() {
			if err != nil {
				status.SetText("Could not load printings: " + err.Error())
				return
			}
			printings = found
			status.SetText(fmt.Sprintf("%s. Pick the one you own; quantity, condition, price paid and date are kept.", plural(len(found), "printing", "printings")))
			list.Refresh()
		})
	}()
}

// finishNames lists finishes by display name, e.g. "Normal, Foil".
func finishNames(finishes []string, g game.Game) string {
	names := make([]string, len(finishes))
	for i, f := range finishes {
		names[i] = g.FinishName(f)
	}
	return strings.Join(names, ", ")
}
