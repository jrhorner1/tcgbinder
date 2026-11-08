package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"github.com/jrhorner1/mtgmanager/internal/service"
)

// New creates the UI for the given games (each already opened, with its own files),
// shown in window. The first game starts active.
func New(window fyne.Window, games ...*service.Game) *App {
	ui := &App{window: window, statusLabel: widget.NewLabel("System Ready")}
	for _, svc := range games {
		ui.games = append(ui.games, &gameState{svc: svc, game: svc.Game, store: svc.Store, prices: svc.Prices})
	}
	if len(ui.games) > 0 {
		ui.g = ui.games[0]
	}
	return ui
}

// BuildLayout constructs the main window content: side-by-side tree and page on
// desktop, or a list screen that pushes page screens on phones.
func (ui *App) BuildLayout() fyne.CanvasObject {
	ui.rightPane = container.NewStack()

	// Filters above the Home / Collection / set / card tree, Add at the bottom.
	filterBar := ui.buildFilterBar()
	ui.tree = ui.buildTree()
	ui.RefreshDataIndex()
	addButton := widget.NewButtonWithIcon("Add", theme.ContentAddIcon(), ui.showAddDialog)
	addButton.Importance = widget.HighImportance
	leftPane := container.NewBorder(filterBar, container.NewPadded(addButton), nil, nil, ui.tree)

	topBar := container.NewHBox(
		widget.NewIcon(theme.StorageIcon()),
		widget.NewLabelWithStyle("MTG Manager", fyne.TextAlignLeading, fyne.TextStyle{Bold: true}),
		layout.NewSpacer(),
		ui.ImportButton(ui.window),
	)
	statusBar := container.NewVBox(widget.NewSeparator(), ui.statusLabel)

	if isMobile() {
		ui.listView = container.NewBorder(topBar, nil, nil, nil, leftPane)
		back := widget.NewButtonWithIcon("Back", theme.NavigateBackIcon(), ui.showList)
		back.Importance = widget.LowImportance
		ui.detailTitle = widget.NewLabelWithStyle("", fyne.TextAlignCenter, fyne.TextStyle{Bold: true})
		ui.detailTitle.Truncation = fyne.TextTruncateEllipsis
		header := container.NewVBox(container.NewBorder(nil, nil, back, nil, ui.detailTitle), widget.NewSeparator())
		ui.detailView = container.NewBorder(header, nil, nil, nil, ui.rightPane)
		ui.mobileRoot = container.NewStack(ui.listView)
		ui.tree.Select(nodeHome) // open on the dashboard; Back shows the collection
		return container.NewBorder(nil, statusBar, nil, nil, ui.mobileRoot)
	}

	splitLayout := container.NewHSplit(leftPane, ui.rightPane)
	splitLayout.Offset = 0.38
	ui.tree.Select(nodeHome) // start on the dashboard
	return container.NewBorder(topBar, statusBar, nil, nil, splitLayout)
}

// showDetail switches the phone layout to the page screen.
func (ui *App) showDetail(title string) {
	if ui.mobileRoot == nil {
		return
	}
	ui.detailTitle.SetText(title)
	ui.mobileRoot.Objects = []fyne.CanvasObject{ui.detailView}
	ui.mobileRoot.Refresh()
}

// showList returns the phone layout to the collection list.
func (ui *App) showList() {
	if ui.mobileRoot == nil {
		return
	}
	ui.page = ""
	ui.tree.UnselectAll() // so tapping the same row again reopens it
	ui.mobileRoot.Objects = []fyne.CanvasObject{ui.listView}
	ui.mobileRoot.Refresh()
}
