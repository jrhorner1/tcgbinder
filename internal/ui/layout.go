package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"github.com/jrhorner1/tcgbinder/internal/service"
	"github.com/jrhorner1/tcgbinder/internal/settings"
)

// New creates the UI for the given games (each already opened, with its own files),
// shown in window, with the user's settings. The first game starts active.
func New(window fyne.Window, prefs *settings.Store, games ...*service.Game) *App {
	if prefs == nil {
		prefs = settings.Memory()
	}
	ui := &App{window: window, settings: prefs, statusLabel: widget.NewLabel("System Ready")}
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
	// Pages draw some text and shapes in theme colors looked up when built, so redraw
	// when light/dark mode changes (on iOS the mode is known only after startup).
	ui.applyTheme(ui.settings.Get()) // before the listener: nothing to redraw yet
	fyne.CurrentApp().Settings().AddListener(func(fyne.Settings) {
		runOnUI(ui.refreshCurrentPage)
	})
	ui.settings.OnChange(func(old, new settings.Settings) {
		runOnUI(func() { ui.settingsChanged(old, new) })
	})
	ui.rightPane = container.NewStack()

	// Filters above the Home / Tools / Collection / Settings tree, Add at the bottom.
	filterBar := ui.buildFilterBar()
	ui.tree = ui.buildTree()
	ui.RefreshDataIndex()
	addButton := widget.NewButtonWithIcon("Add", theme.ContentAddIcon(), ui.showAddDialog)
	addButton.Importance = widget.HighImportance
	var bottom fyne.CanvasObject = addButton
	if ui.canScanCamera() || ui.canScanPhoto() { // Scan beside Add: camera on phones, a photo on desktop
		scanButton := widget.NewButtonWithIcon("Scan", theme.MediaPhotoIcon(), func() {
			if ui.canScanCamera() {
				ui.startScan(false)
			} else {
				ui.scanPhotoFile()
			}
		})
		scanButton.Importance = widget.HighImportance
		bottom = container.NewGridWithColumns(2, addButton, scanButton)
	}
	ui.selection = ui.newSelectionBar()
	ui.updateSelection()
	leftPane := container.NewBorder(filterBar, container.NewVBox(ui.selection.view, container.NewPadded(bottom)), nil, nil, ui.tree)

	brand := container.NewHBox(widget.NewIcon(theme.StorageIcon()),
		widget.NewLabelWithStyle("TCG Binder", fyne.TextAlignLeading, fyne.TextStyle{Bold: true}))
	search := ui.globalSearchEntry()
	var topBar fyne.CanvasObject
	if isMobile() { // search gets its own row on narrow screens
		topBar = container.NewVBox(brand, search)
	} else {
		topBar = container.NewBorder(nil, nil, brand,
			container.NewGridWrap(fyne.NewSize(260, search.MinSize().Height), search))
	}
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
