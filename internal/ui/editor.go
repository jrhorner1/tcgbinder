package ui

import (
	"fmt"
	"log/slog"
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
	"github.com/jrhorner1/mtgmanager/internal/collection"
	"github.com/jrhorner1/mtgmanager/internal/imagecache"
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

func conditionLabel(c collection.Condition) string {
	for _, l := range conditionLabels {
		if l.Code == c {
			return l.Label
		}
	}
	return string(c)
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
func marketPricesText(c collection.Card, finishes []string) string {
	if len(c.Market) == 0 {
		return "not available yet"
	}
	var parts []string
	seen := map[string]bool{}
	for _, f := range append(append([]string{}, finishes...), c.Finishes...) {
		if p, ok := c.Market[f]; ok && !seen[f] {
			seen[f] = true
			parts = append(parts, formatUSD(p)+" "+f)
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

// selectCardNode shows an entry of the active game in the tree and opens it in the editor.
func (ui *App) selectCardNode(c collection.Card) { ui.selectEntry(collection.EntryKey(c), c.Set) }

// selectEntry is selectCardNode by entry key.
func (ui *App) selectEntry(key, set string) {
	gameID := ui.g.game.ID()
	id := prefixCard + gameID + ":" + key
	ui.tree.OpenBranch(gameNode(gameID))
	ui.tree.OpenBranch(setNode(gameID, strings.ToUpper(set)))
	ui.tree.UnselectAll() // so selecting the same node again still fires OnSelected
	ui.tree.Select(id)
	ui.tree.ScrollTo(id)
}

// renderEditor shows one collection entry: its image, value, editable ownership
// details, card information and other copies of the same card.
func (ui *App) renderEditor(card collection.Card) {
	ui.page = "card"
	ui.editorGen++
	gen := ui.editorGen
	oldKey := collection.EntryKey(card)

	// --- Image (loaded in the background) ---
	img := canvas.NewImageFromResource(theme.DownloadIcon())
	img.FillMode = canvas.ImageFillContain
	img.SetMinSize(fyne.NewSize(250, 349)) // card aspect ratio
	if card.ImageURI == "" {
		img.Resource = theme.BrokenImageIcon()
	} else {
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
	subParts = append(subParts, setNumber(card.Set, card.Number))
	if card.Rarity != "" {
		subParts = append(subParts, titleCase(card.Rarity))
	}
	header := container.NewVBox(pageTitle(card.Name), caption(strings.Join(subParts, " · ")))

	// --- Value tiles (saved state) ---
	qty := float64(card.Quantity)
	each, priced := collection.MarketPrice(card)
	marketValue, marketDetail := "—", "no market price for "+card.Finish
	if priced {
		marketValue = formatUSD(each * qty)
		marketDetail = formatUSD(each) + " each"
	}
	paidValue, paidDetail := "—", "price paid not recorded"
	if card.PurchasePrice > 0 {
		paidValue = formatUSD(card.PurchasePrice * qty)
		paidDetail = formatUSD(card.PurchasePrice) + " each"
	}
	gainValue, gainDetail, gainColor := "—", "needs both prices", theme.Color(theme.ColorNameForeground)
	if priced && card.PurchasePrice > 0 {
		gainValue, gainDetail, gainColor = gainLoss(card.PurchasePrice*qty, each*qty)
	}
	tiles := container.NewGridWithColumns(cols(3),
		statTile("MARKET VALUE", marketValue, marketDetail, colorMarket),
		statTile("PRICE PAID", paidValue, paidDetail, nil),
		statTile("GAIN / LOSS", gainValue, gainDetail, gainColor),
	)

	// --- Editable ownership details ---
	qtyEntry := widget.NewEntry()
	qtyEntry.SetText(strconv.Itoa(card.Quantity))
	step := func(d int) {
		n, err := strconv.Atoi(strings.TrimSpace(qtyEntry.Text))
		if err != nil {
			n = card.Quantity
		}
		qtyEntry.SetText(strconv.Itoa(max(n+d, 0)))
	}
	qtyRow := container.NewBorder(nil, nil,
		widget.NewButtonWithIcon("", theme.ContentRemoveIcon(), func() { step(-1) }),
		widget.NewButtonWithIcon("", theme.ContentAddIcon(), func() { step(1) }),
		qtyEntry)

	finishes := card.Finishes
	if len(finishes) == 0 {
		finishes = []string{card.Finish}
	}
	finishSelect := widget.NewSelect(finishes, nil)
	finishSelect.SetSelected(card.Finish)

	condOptions := make([]string, len(conditionLabels))
	for i, l := range conditionLabels {
		condOptions[i] = l.Label
	}
	condSelect := widget.NewSelect(condOptions, nil)
	condSelect.SetSelected(conditionLabel(card.Condition))

	priceEntry := widget.NewEntry()
	priceEntry.SetPlaceHolder("0.00")
	if card.PurchasePrice > 0 {
		priceEntry.SetText(strconv.FormatFloat(card.PurchasePrice, 'f', 2, 64))
	}
	dateEntry := widget.NewEntry()
	dateEntry.SetPlaceHolder("YYYY-MM-DD")
	dateEntry.SetText(card.AcquiredDate)

	errorLabel := widget.NewLabel("")
	errorLabel.Importance = widget.DangerImportance
	errorLabel.Wrapping = fyne.TextWrapWord
	errorLabel.Hide()

	// read parses the form; it returns the edited entry or a message for the user.
	read := func() (collection.Card, string) {
		edited := card
		n, err := strconv.Atoi(strings.TrimSpace(qtyEntry.Text))
		if err != nil || n < 0 {
			return edited, "Quantity must be a whole number (0 or more)."
		}
		edited.Quantity = n
		edited.Finish = finishSelect.Selected
		edited.Condition = conditionFromLabel(condSelect.Selected)
		p := strings.TrimPrefix(strings.TrimSpace(priceEntry.Text), "$")
		edited.PurchasePrice = 0
		if p != "" {
			v, err := strconv.ParseFloat(p, 64)
			if err != nil || v < 0 {
				return edited, "Price paid must be a number, e.g. 2.50."
			}
			edited.PurchasePrice = v
		}
		d := strings.TrimSpace(dateEntry.Text)
		if d != "" {
			if _, err := time.Parse("2006-01-02", d); err != nil {
				return edited, "Acquired date must look like 2024-03-15."
			}
		}
		edited.AcquiredDate = d
		return edited, ""
	}

	saveButton := widget.NewButtonWithIcon("Save Changes", theme.DocumentSaveIcon(), nil)
	saveButton.Importance = widget.HighImportance
	revertButton := widget.NewButtonWithIcon("Revert", theme.ContentUndoIcon(), nil)
	deleteButton := widget.NewButtonWithIcon("Delete", theme.DeleteIcon(), nil)
	deleteButton.Importance = widget.DangerImportance

	// Save and Revert are only enabled while the form differs from what's saved.
	updateDirty := func() {
		edited, _ := read()
		dirty := strings.TrimSpace(qtyEntry.Text) != strconv.Itoa(card.Quantity) ||
			edited.Finish != card.Finish || edited.Condition != card.Condition ||
			strings.TrimSpace(priceEntry.Text) != strings.TrimSpace(func() string {
				if card.PurchasePrice > 0 {
					return strconv.FormatFloat(card.PurchasePrice, 'f', 2, 64)
				}
				return ""
			}()) ||
			strings.TrimSpace(dateEntry.Text) != card.AcquiredDate
		if dirty {
			saveButton.Enable()
			revertButton.Enable()
		} else {
			saveButton.Disable()
			revertButton.Disable()
		}
	}
	qtyEntry.OnChanged = func(string) { updateDirty() }
	priceEntry.OnChanged = func(string) { updateDirty() }
	dateEntry.OnChanged = func(string) { updateDirty() }
	finishSelect.OnChanged = func(string) { updateDirty() }
	condSelect.OnChanged = func(string) { updateDirty() }
	updateDirty()

	deleteEntry := func() {
		dialog.ShowConfirm("Delete from collection?",
			fmt.Sprintf("Remove all %d of %s (%s, %s, %s) from your collection?",
				card.Quantity, card.Name, setNumber(card.Set, card.Number), card.Finish, card.Condition),
			func(ok bool) {
				if !ok {
					return
				}
				if err := ui.g.store.RemoveCard(oldKey); err != nil {
					dialog.ShowError(fmt.Errorf("delete failed: %w", err), ui.window)
					return
				}
				ui.statusLabel.SetText(fmt.Sprintf("Deleted %s (%s)", card.Name, setNumber(card.Set, card.Number)))
				ui.RefreshDataIndex()
				ui.tree.UnselectAll()
				ui.tree.Select(setNode(ui.g.game.ID(), strings.ToUpper(card.Set)))
			}, ui.window)
	}
	deleteButton.OnTapped = deleteEntry

	save := func() {
		edited, problem := read()
		if problem != "" {
			errorLabel.SetText(problem)
			errorLabel.Show()
			return
		}
		errorLabel.Hide()
		if edited.Quantity == 0 {
			deleteEntry()
			return
		}
		newKey, merged, err := ui.g.store.UpdateEntry(oldKey, edited)
		if err != nil {
			dialog.ShowError(fmt.Errorf("save failed: %w", err), ui.window)
			return
		}
		msg := fmt.Sprintf("Saved %s", card.Name)
		if merged {
			msg = fmt.Sprintf("Saved %s — merged into your existing %s %s copies", card.Name, edited.Finish, edited.Condition)
		}
		ui.statusLabel.SetText(msg)
		ui.RefreshDataIndex()
		ui.selectEntry(newKey, edited.Set)
	}
	saveButton.OnTapped = save
	qtyEntry.OnSubmitted = func(string) { save() }
	priceEntry.OnSubmitted = func(string) { save() }
	dateEntry.OnSubmitted = func(string) { save() }
	revertButton.OnTapped = func() { ui.renderEditor(card) }

	ownership := widget.NewForm(
		widget.NewFormItem("Quantity", qtyRow),
		widget.NewFormItem("Finish", finishSelect),
		widget.NewFormItem("Condition", condSelect),
		widget.NewFormItem("Price paid (each)", priceEntry),
		widget.NewFormItem("Acquired", dateEntry),
	)
	buttons := container.NewBorder(nil, nil, deleteButton, container.NewHBox(revertButton, saveButton))

	// --- Card details ---
	typeLabel := widget.NewLabel(card.TypeLine)
	typeLabel.TextStyle = fyne.TextStyle{Bold: true}
	typeLabel.Wrapping = fyne.TextWrapWord
	textLabel := widget.NewLabel(card.Text)
	textLabel.Wrapping = fyne.TextWrapWord
	if card.Text == "" {
		// Entries imported before rules text was stored: fetch it once and keep it.
		textLabel.SetText("Loading rules text…")
		go func() {
			typeLine, text, err := ui.g.game.CardText(card.ID)
			runOnUI(func() {
				if gen != ui.editorGen {
					return
				}
				if err != nil {
					textLabel.SetText("Rules text unavailable.")
					return
				}
				ui.g.store.SetCatalogText(card.ID, typeLine, text)
				typeLabel.SetText(typeLine)
				textLabel.SetText(text)
			})
		}()
	}

	detailItems := []*widget.FormItem{
		infoRow("Set", fmt.Sprintf("%s (%s)", card.SetName, card.Set)),
		infoRow("Collector number", card.Number),
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
		infoRow("Printed in", strings.Join(card.Finishes, ", ")),
		infoRow("Market prices", marketPricesText(card, ui.g.game.Finishes())),
	)

	right := container.NewVBox(
		tiles,
		sectionTitle("Your Copies"),
		ownership,
		errorLabel,
		buttons,
		sectionTitle("Card"),
		typeLabel,
		textLabel,
		widget.NewForm(detailItems...),
	)

	// --- Other copies of the same card (other printings, finishes or conditions) ---
	var others []collection.Card
	for _, c := range ui.g.cards {
		if collection.EntryKey(c) == oldKey {
			continue
		}
		if c.Name == card.Name { // other printings, finishes or conditions of the same card
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
		right.Add(sectionTitle("Other Copies You Own"))
		for _, o := range others {
			o := o
			parts := []string{setNumber(o.Set, o.Number)}
			if o.SetName != "" {
				parts[0] = o.SetName + " · " + parts[0]
			}
			parts = append(parts, o.Finish, string(o.Condition), fmt.Sprintf("×%d", o.Quantity))
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
