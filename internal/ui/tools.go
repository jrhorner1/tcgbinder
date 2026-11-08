package ui

import (
	"fmt"
	"strconv"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"github.com/jrhorner1/tcgbinder/internal/collection"
)

// globalSearchEntry searches every game's collection (top right of the window).
func (ui *App) globalSearchEntry() *widget.Entry {
	e := widget.NewEntry()
	e.SetPlaceHolder("Search all cards")
	e.ActionItem = widget.NewIcon(theme.SearchIcon())
	e.OnSubmitted = func(q string) {
		if strings.TrimSpace(q) != "" {
			ui.showSearchResults(strings.TrimSpace(q))
			ui.showDetail("Search")
		}
	}
	return e
}

// showSearchResults lists owned cards matching q in every game, and offers a catalog
// search for cards you don't own yet.
func (ui *App) showSearchResults(q string) {
	ui.page = "search:" + q
	content := container.NewVBox(container.NewPadded(pageTitle(fmt.Sprintf("Search: %q", q))))
	total := 0
	for _, gs := range ui.games {
		gs := gs
		matches := collection.ApplyQuery(gs.cards, collection.Query{Text: q, SortBy: collection.SortName})
		total += len(matches)
		if len(ui.games) > 1 || len(matches) > 0 {
			content.Add(sectionTitle(fmt.Sprintf("%s — %s", gs.game.Name(), plural(len(matches), "match", "matches"))))
		}
		const limit = 200
		for i, c := range matches {
			if i == limit {
				content.Add(caption(fmt.Sprintf("Showing the first %d; refine the search to see more.", limit)))
				break
			}
			c := c
			parts := []string{setNumber(c.Set, c.CollectorNumber)}
			parts = append(parts, copiesSummary(c, gs.game))
			name := widget.NewLabel(c.Name)
			name.Truncation = fyne.TextTruncateEllipsis
			row := container.NewBorder(nil, nil, nil, widget.NewLabel(strings.Join(parts, " · ")), name)
			content.Add(newTapRow(row, func() {
				ui.activate(gs)
				ui.selectCardNode(c)
			}))
		}
		content.Add(widget.NewButtonWithIcon(fmt.Sprintf("Search the %s catalog for %q", gs.game.Name(), q), theme.ContentAddIcon(), func() {
			ui.activate(gs)
			ui.showAddDialogFor(q, "")
		}))
	}
	if total == 0 {
		content.Objects = append(content.Objects[:1], append([]fyne.CanvasObject{caption("No cards in your collection match. Search the catalog to add one.")}, content.Objects[1:]...)...)
	}
	ui.setRightPane(container.NewVScroll(container.NewPadded(content)))
}

// showBulkEdit changes many cards at once: those the filters currently show, with
// any of them unticked to leave out.
func (ui *App) showBulkEdit() {
	gs := ui.g
	cards := append([]collection.Card(nil), gs.filtered...)
	if len(cards) == 0 {
		dialog.ShowInformation("Bulk edit", "No cards are shown. Change the filters to choose the cards to edit.", ui.window)
		return
	}
	chosen := make([]bool, len(cards))
	for i := range chosen {
		chosen[i] = true
	}
	count := widget.NewLabel("")
	updateCount := func() {
		n := 0
		for _, c := range chosen {
			if c {
				n++
			}
		}
		count.SetText(fmt.Sprintf("%d of %s selected (the cards the filters show)", n, plural(len(cards), "card", "cards")))
	}
	list := widget.NewList(
		func() int { return len(cards) },
		func() fyne.CanvasObject {
			check := widget.NewCheck("", nil)
			detail := widget.NewLabel("")
			return container.NewBorder(nil, nil, nil, detail, check)
		},
		func(id widget.ListItemID, obj fyne.CanvasObject) {
			row := obj.(*fyne.Container)
			check := row.Objects[0].(*widget.Check)
			c := cards[id]
			check.OnChanged = nil
			check.SetText(c.Name)
			check.SetChecked(chosen[id])
			check.OnChanged = func(v bool) { chosen[id] = v; updateCount() }
			parts := []string{setNumber(c.Set, c.CollectorNumber), copiesSummary(c, gs.game)}
			row.Objects[1].(*widget.Label).SetText(strings.Join(parts, " · "))
			row.Refresh()
		},
	)
	setAll := func(v bool) {
		for i := range chosen {
			chosen[i] = v
		}
		list.Refresh()
		updateCount()
	}
	updateCount()

	const unchanged = "(unchanged)"
	condOptions := []string{unchanged}
	for _, l := range conditionLabels {
		condOptions = append(condOptions, l.Label)
	}
	cond := widget.NewSelect(condOptions, nil)
	cond.SetSelected(unchanged)
	finishIDs := map[string]string{}
	finishOptions := []string{unchanged}
	for _, f := range gs.game.Finishes() {
		finishIDs[gs.game.FinishName(f)] = f
		finishOptions = append(finishOptions, gs.game.FinishName(f))
	}
	finish := widget.NewSelect(finishOptions, nil)
	finish.SetSelected(unchanged)
	price := widget.NewEntry()
	price.SetPlaceHolder(unchanged)
	date := widget.NewDateEntry()
	date.SetPlaceHolder(unchanged)
	del := widget.NewCheck("Delete the selected cards", nil)

	var d *dialog.CustomDialog
	apply := widget.NewButtonWithIcon("Apply", theme.ConfirmIcon(), func() {
		var keys []string
		for i, c := range cards {
			if chosen[i] {
				keys = append(keys, collection.CardKey(c))
			}
		}
		if len(keys) == 0 {
			return
		}
		var change collection.BulkChange
		var what []string
		if cond.Selected != unchanged {
			c := conditionFromLabel(cond.Selected)
			change.Condition = &c
			what = append(what, "condition "+string(c))
		}
		if finish.Selected != unchanged {
			f := finishIDs[finish.Selected]
			change.Finish = &f
			what = append(what, "finish "+finish.Selected)
		}
		if p := strings.TrimPrefix(strings.TrimSpace(price.Text), "$"); p != "" {
			v, err := strconv.ParseFloat(p, 64)
			if err != nil || v < 0 {
				dialog.ShowError(fmt.Errorf("price paid must be a number, e.g. 2.50"), ui.window)
				return
			}
			change.PricePaid = &v
			what = append(what, "price paid "+formatUSD(v))
		}
		if dv := pickedDate(date); dv != "" {
			change.AcquiredDate = &dv
			what = append(what, "acquired "+dv)
		}
		change.Delete = del.Checked
		if !change.Delete && len(what) == 0 {
			dialog.ShowInformation("Bulk edit", "Choose at least one change.", ui.window)
			return
		}
		question := fmt.Sprintf("Set %s on %s?", strings.Join(what, ", "), plural(len(keys), "card", "cards"))
		if change.Delete {
			question = fmt.Sprintf("Delete %s from your collection? This can't be undone.", plural(len(keys), "card", "cards"))
		}
		run := func() {
			res, err := gs.store.BulkEdit(keys, change)
			if err != nil {
				dialog.ShowError(fmt.Errorf("bulk edit failed: %w", err), ui.window)
				return
			}
			d.Hide()
			msg := fmt.Sprintf("Updated %s", plural(res.Updated, "card", "cards"))
			if res.Deleted > 0 {
				msg = fmt.Sprintf("Deleted %s", plural(res.Deleted, "card", "cards"))
			}
			if len(res.Skipped) > 0 {
				msg += fmt.Sprintf("; %d skipped (finish not available)", len(res.Skipped))
			}
			ui.statusLabel.SetText(msg)
			ui.RefreshDataIndex()
		}
		if change.Delete {
			ui.confirm("Bulk edit", question, run) // the Ask before deleting setting applies
		} else {
			dialog.ShowConfirm("Bulk edit", question, func(ok bool) {
				if ok {
					run()
				}
			}, ui.window)
		}
	})
	apply.Importance = widget.HighImportance

	changes := widget.NewForm(
		widget.NewFormItem("Condition", cond),
		widget.NewFormItem("Finish", finish),
		widget.NewFormItem("Price paid (each)", price),
		widget.NewFormItem("Acquired", date),
		widget.NewFormItem("", del),
	)
	top := container.NewVBox(count, container.NewHBox(
		widget.NewButton("Select all", func() { setAll(true) }),
		widget.NewButton("Select none", func() { setAll(false) })))
	bottom := container.NewVBox(widget.NewSeparator(), changes,
		caption("Changes apply to every copy of each selected card. Finish changes skip printings that don't come in that finish; copies that end up the same finish, condition, price and date join one purchase."))
	content := container.NewBorder(top, bottom, nil, nil, list)
	d = dialog.NewCustomWithoutButtons("Bulk edit "+gs.game.Name(), content, ui.window)
	d.SetButtons([]fyne.CanvasObject{widget.NewButton("Cancel", func() { d.Hide() }), apply})
	size := fyne.NewSize(760, 680)
	if isMobile() {
		size = ui.window.Canvas().Size()
	}
	d.Resize(size)
	d.Show()
}
