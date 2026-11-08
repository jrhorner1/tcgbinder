// Package ui is the Fyne user interface: the collection tree, dashboard, card editor,
// and add/import dialogs. It works only through the game.Game interface and never
// imports a particular game.
package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/widget"
)

// runOnUI schedules f on the UI goroutine. It is fyne.Do in the app; tests replace it
// because Fyne's test driver runs fyne.Do inline on the calling goroutine.
var runOnUI = fyne.Do

// isMobile reports whether to use the phone layout. Tests override it.
var isMobile = func() bool { return fyne.CurrentDevice().IsMobile() }

// cols caps a grid's column count on phones, where screens are ~390pt wide.
func cols(desktop int) int {
	if isMobile() {
		return min(desktop, 2)
	}
	return desktop
}

// App is the running desktop/mobile UI: collection tree, pages and dialogs.
type App struct {
	window      fyne.Window
	statusLabel *widget.Label
	tree        *widget.Tree
	rightPane   *fyne.Container
	view        collectionView
	editorGen   int    // bumped each time the editor is rebuilt; only touched on the UI thread
	page        string // what the right pane shows: "home", "set:<code>", "card"
	selected    string // tree node currently selected

	games []*gameState // one per game, each with its own files
	g     *gameState   // the active game: the one Add, Import and the filters apply to

	// Phone layout: one screen at a time, list or detail.
	mobileRoot  *fyne.Container
	listView    fyne.CanvasObject
	detailView  fyne.CanvasObject
	detailTitle *widget.Label
}

// RefreshDataIndex reloads every game's entries from its store and refreshes the view.
func (ui *App) RefreshDataIndex() {
	for _, gs := range ui.games {
		gs.cards = gs.store.GetCollection()
	}
	ui.refreshSetOptions()
	ui.applyFilter()
	ui.refreshCurrentPage()
}
