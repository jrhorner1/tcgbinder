package ui

import (
	"fmt"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"github.com/jrhorner1/tcgbinder/internal/collection"
)

// Ticking cards for bulk edit: each card row in the tree has a checkbox. The search
// and filters above the tree narrow what's shown, "Select all shown" ticks it, and
// Bulk edit changes every ticked card.

// selectionBar sits above Add: how many cards are ticked, and what to do with them.
type selectionBar struct {
	view      *fyne.Container
	count     *widget.Label
	selectAll *widget.Button
	clear     *widget.Button
	bulkEdit  *widget.Button
}

func (ui *App) newSelectionBar() *selectionBar {
	b := &selectionBar{count: widget.NewLabel("")}
	b.count.TextStyle = fyne.TextStyle{Italic: true}
	b.selectAll = widget.NewButtonWithIcon("Select all shown", theme.CheckButtonCheckedIcon(), func() { ui.tickShown(true) })
	b.clear = widget.NewButtonWithIcon("Clear", theme.ContentClearIcon(), func() { ui.tickAll(false) })
	b.bulkEdit = widget.NewButtonWithIcon("Bulk edit…", theme.DocumentCreateIcon(), ui.showBulkEdit)
	b.bulkEdit.Importance = widget.HighImportance
	buttons := container.NewHBox(b.selectAll, b.clear, b.bulkEdit)
	if isMobile() { // narrow: the count above the buttons, which drop their icons to fit
		for _, btn := range []*widget.Button{b.selectAll, b.clear, b.bulkEdit} {
			btn.SetIcon(nil)
		}
		b.view = container.NewVBox(widget.NewSeparator(), b.count, container.NewGridWithColumns(3, b.selectAll, b.clear, b.bulkEdit))
	} else {
		b.view = container.NewVBox(widget.NewSeparator(), container.NewBorder(nil, nil, b.count, nil, container.NewHBox(layout.NewSpacer(), buttons)))
	}
	return b
}

// update shows the active game's ticked count and enables the buttons that apply.
func (ui *App) updateSelection() {
	b := ui.selection
	if b == nil {
		return
	}
	n, shown := len(ui.g.ticked), 0
	for _, c := range ui.g.filtered {
		if ui.g.ticked[collection.CardKey(c)] {
			shown++
		}
	}
	switch {
	case n == 0:
		b.count.SetText("Tick cards to bulk edit them")
	case shown < n:
		b.count.SetText(fmt.Sprintf("%d ticked (%d hidden by filters)", n, n-shown))
	default:
		b.count.SetText(fmt.Sprintf("%d ticked", n))
	}
	if len(ui.g.filtered) == 0 {
		b.selectAll.Disable()
	} else {
		b.selectAll.Enable()
	}
	for _, btn := range []*widget.Button{b.clear, b.bulkEdit} {
		if n == 0 {
			btn.Disable()
		} else {
			btn.Enable()
		}
	}
}

// tick ticks or unticks one card of gs.
func (ui *App) tick(gs *gameState, key string, on bool) {
	if gs.ticked == nil {
		gs.ticked = map[string]bool{}
	}
	if on {
		gs.ticked[key] = true
	} else {
		delete(gs.ticked, key)
	}
	ui.updateSelection()
}

// tickShown ticks (or unticks) every card the search and filters show.
func (ui *App) tickShown(on bool) {
	for _, c := range ui.g.filtered {
		ui.tick(ui.g, collection.CardKey(c), on)
	}
	ui.tree.Refresh()
}

// tickAll unticks (or ticks) every card of the active game, shown or not.
func (ui *App) tickAll(on bool) {
	ui.g.ticked = map[string]bool{}
	if on {
		for _, c := range ui.g.cards {
			ui.g.ticked[collection.CardKey(c)] = true
		}
	}
	ui.tree.Refresh()
	ui.updateSelection()
}

// tickedCards are the active game's ticked cards, in the tree's order.
func (ui *App) tickedCards() []collection.Card {
	var out []collection.Card
	for _, c := range collection.SortCards(ui.g.cards, ui.settings.Get().CardOrder) {
		if ui.g.ticked[collection.CardKey(c)] {
			out = append(out, c)
		}
	}
	return out
}

// pruneTicks forgets ticks for cards no longer in the collection.
func (gs *gameState) pruneTicks() {
	if len(gs.ticked) == 0 {
		return
	}
	have := make(map[string]bool, len(gs.cards))
	for _, c := range gs.cards {
		have[collection.CardKey(c)] = true
	}
	for k := range gs.ticked {
		if !have[k] {
			delete(gs.ticked, k)
		}
	}
}
