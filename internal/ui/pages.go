package ui

import (
	"fmt"
	"image/color"
	"log/slog"
	"net/url"
	"path/filepath"
	"slices"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"github.com/jrhorner1/tcgbinder/internal/settings"
	"github.com/jrhorner1/tcgbinder/internal/version"
)

// confirm asks before a destructive action, unless the user turned that off.
func (ui *App) confirm(title, question string, do func()) {
	if !ui.settings.Get().ConfirmDelete {
		do()
		return
	}
	dialog.ShowConfirm(title, question, func(ok bool) {
		if ok {
			do()
		}
	}, ui.window)
}

// --- Theme ----------------------------------------------------------------------

// appTheme is Fyne's theme with the user's light/dark choice and text size.
type appTheme struct {
	variant *fyne.ThemeVariant // nil follows the system
	scale   float32
}

func (t appTheme) Color(n fyne.ThemeColorName, v fyne.ThemeVariant) color.Color {
	if t.variant != nil {
		v = *t.variant
	}
	return theme.DefaultTheme().Color(n, v)
}
func (t appTheme) Font(s fyne.TextStyle) fyne.Resource     { return theme.DefaultTheme().Font(s) }
func (t appTheme) Icon(n fyne.ThemeIconName) fyne.Resource { return theme.DefaultTheme().Icon(n) }
func (t appTheme) Size(n fyne.ThemeSizeName) float32 {
	return theme.DefaultTheme().Size(n) * t.scale
}

func themeFor(s settings.Settings) fyne.Theme {
	t := appTheme{scale: s.TextScale()}
	switch s.Theme {
	case "light":
		v := theme.VariantLight
		t.variant = &v
	case "dark":
		v := theme.VariantDark
		t.variant = &v
	}
	return t
}

// applyTheme installs the theme for the settings. Fyne then redraws every widget, and
// the settings listener in BuildLayout rebuilds the current page's custom drawing.
func (ui *App) applyTheme(s settings.Settings) {
	fyne.CurrentApp().Settings().SetTheme(themeFor(s))
}

// settingsChanged applies changed settings to the running app.
func (ui *App) settingsChanged(old, new settings.Settings) {
	if old.Theme != new.Theme || old.TextSize != new.TextSize {
		ui.applyTheme(new)
	}
	if old.CardOrder != new.CardOrder {
		ui.applyFilter()
	}
	if old.PriceRefresh != new.PriceRefresh {
		for _, gs := range ui.games {
			if gs.svc.PricesDue() {
				ui.refreshPrices(gs)
			}
		}
	}
}

// --- Tools ----------------------------------------------------------------------

// toolAction is one card on the Tools page: a button and what it does.
func toolAction(label string, icon fyne.Resource, about string, run func()) fyne.CanvasObject {
	b := widget.NewButtonWithIcon(label, icon, run)
	b.Alignment = widget.ButtonAlignLeading
	text := caption(about)
	text.TextStyle = fyne.TextStyle{}
	if isMobile() {
		return container.NewVBox(b, text)
	}
	return container.NewBorder(nil, nil, container.NewGridWrap(fyne.NewSize(240, b.MinSize().Height), b), nil, text)
}

// showTools shows the Tools page: import/export, bulk edit and price updates for the
// active game.
func (ui *App) showTools() {
	ui.page = "tools"
	gs := ui.g
	content := container.NewVBox(container.NewPadded(pageTitle("Tools")))
	if len(ui.games) > 1 {
		content.Add(caption("For " + gs.game.Name() + ". Select a game in the list to switch."))
	}

	content.Add(sectionTitle("Collection"))
	content.Add(toolAction("Import…", theme.UploadIcon(),
		"Add cards from another app's CSV export (EchoMTG, ManaBox, Moxfield, Deckbox, TCGplayer and others) or from a collection file exported here.",
		ui.startImport))
	content.Add(toolAction("Export collection…", theme.DownloadIcon(),
		"Save the collection file, for a backup or to move to another device. Import reads it back.",
		ui.exportCollection))
	if ui.canScanCamera() {
		content.Add(toolAction("Scan cards", theme.MediaPhotoIcon(),
			"Point the camera at a card: it's read and matched by its name, set and collector number. Add it, then scan the next one.",
			func() { ui.startScan(false) }))
		content.Add(toolAction("Scan from a photo", theme.FileImageIcon(),
			"Identify a card from a photo in your library.",
			func() { ui.startScan(true) }))
	} else if ui.canScanPhoto() {
		content.Add(toolAction("Identify from photo…", theme.MediaPhotoIcon(),
			"Pick a photo of a card: it's read and matched by its name, set and collector number, ready to add.",
			ui.scanPhotoFile))
	}
	content.Add(toolAction("Bulk edit…", theme.DocumentCreateIcon(),
		"Change the condition, finish, price paid or date of many cards at once, or delete them. Tick the cards in the collection list (filter it, then Select all shown); Bulk edit is also below the list.",
		ui.showBulkEdit))

	content.Add(sectionTitle("Prices"))
	asOf := ""
	for _, c := range gs.cards {
		if c.PricesUpdated > asOf {
			asOf = c.PricesUpdated
		}
	}
	pricesFrom := "No prices have been fetched yet"
	if asOf != "" {
		pricesFrom = "Prices are from " + asOf
	}
	refreshNote := fmt.Sprintf("Fetch today's market prices now. %s; they update %s (see Settings).",
		pricesFrom, map[string]string{"daily": "automatically each day", "weekly": "automatically each week", "manual": "only when you ask"}[ui.settings.Get().PriceRefresh])
	if gs.refreshing {
		refreshNote = "Refreshing prices…"
	}
	refresh := toolAction("Refresh prices now", theme.ViewRefreshIcon(), refreshNote, func() { ui.refreshPrices(gs) })
	content.Add(refresh)
	if src, ok := gs.svc.PriceHistorySource(); ok {
		label, note := "Load price history", "Download "+src.PriceHistoryInfo()+" so the value chart shows the past, not just the days the app was used."
		if on := gs.prices.HistoryLoadedOn(); on != "" {
			label, note = "Update price history", fmt.Sprintf("Price history last loaded %s. Download %s again to fill in recent days.", on, src.PriceHistoryInfo())
		}
		if gs.loadingHistory {
			note = "Loading price history…"
		}
		content.Add(toolAction(label, theme.HistoryIcon(), note, func() { ui.loadPriceHistory(gs) }))
	}
	ui.setRightPane(container.NewVScroll(container.NewPadded(content)))
}

// --- Settings -------------------------------------------------------------------

// choice is a setting with fixed values, shown as a menu of friendly names.
type choice struct {
	values, names []string
}

func (c choice) name(v string) string {
	if i := slices.Index(c.values, v); i >= 0 {
		return c.names[i]
	}
	return v
}

func (c choice) value(name string) string {
	if i := slices.Index(c.names, name); i >= 0 {
		return c.values[i]
	}
	return name
}

var (
	themeChoice     = choice{settings.Themes, []string{"Match system", "Light", "Dark"}}
	textSizeChoice  = choice{settings.TextSizes, []string{"Small", "Normal", "Large", "Larger"}}
	cardOrderChoice = choice{settings.CardOrders, []string{"By name", "By set, newest first", "By set, oldest first", "Most valuable first"}}
	refreshChoice   = choice{settings.PriceRefreshes, []string{"Daily", "Weekly", "Only when I ask"}}
	chartChoice     = choice{settings.ChartRanges, []string{"30 days", "90 days", "1 year", "All time"}}
	conditionChoice = choice{settings.Conditions, []string{"NM — Near Mint", "LP — Lightly Played", "MP — Moderately Played", "HP — Heavily Played", "DMG — Damaged"}}
	dateChoice      = choice{settings.DefaultDates, []string{"Today", "Leave blank"}}
	logChoice       = choice{settings.LogLevels, []string{"Debug (most detail)", "Info", "Warnings", "Errors only"}}
)

// showSettings shows the Settings page. Changes are saved and applied at once.
func (ui *App) showSettings() {
	ui.page = "settings"
	s := ui.settings.Get()
	save := func(what string, f func(*settings.Settings)) {
		if err := ui.settings.Update(f); err != nil {
			dialog.ShowError(fmt.Errorf("could not save settings: %w", err), ui.window)
			return
		}
		ui.statusLabel.SetText("Saved: " + what)
	}
	menu := func(c choice, current string, what string, set func(*settings.Settings, string)) *widget.Select {
		sel := widget.NewSelect(c.names, nil)
		sel.SetSelected(c.name(current))
		sel.OnChanged = func(name string) {
			save(what, func(s *settings.Settings) { set(s, c.value(name)) })
		}
		return sel
	}
	check := func(label string, on bool, what string, set func(*settings.Settings, bool)) *widget.Check {
		c := widget.NewCheck(label, nil)
		c.SetChecked(on)
		c.OnChanged = func(v bool) { save(what, func(s *settings.Settings) { set(s, v) }) }
		return c
	}
	form := func(items ...*widget.FormItem) *widget.Form {
		f := widget.NewForm(items...)
		if isMobile() {
			f.Orientation = widget.Vertical
		}
		return f
	}
	item := func(label string, w fyne.CanvasObject, hint string) *widget.FormItem {
		fi := widget.NewFormItem(label, w)
		fi.HintText = hint
		return fi
	}

	appearance := form(
		item("Theme", menu(themeChoice, s.Theme, "theme", func(s *settings.Settings, v string) { s.Theme = v }), "Light, dark, or follow the system setting"),
		item("Text size", menu(textSizeChoice, s.TextSize, "text size", func(s *settings.Settings, v string) { s.TextSize = v }), "Scales text and spacing"),
		item("Card images", check("Show card images", s.ShowImages, "card images", func(s *settings.Settings, v bool) { s.ShowImages = v }), "Turn off to save data on slow or metered connections"),
	)
	collectionForm := form(
		item("Card order", menu(cardOrderChoice, s.CardOrder, "card order", func(s *settings.Settings, v string) { s.CardOrder = v }), "How cards are listed under the collection"),
		item("Value chart", menu(chartChoice, s.ChartRange, "chart range", func(s *settings.Settings, v string) { s.ChartRange = v }), "Range the Value Over Time chart shows"),
	)
	prices := form(
		item("Update prices", menu(refreshChoice, s.PriceRefresh, "price updates", func(s *settings.Settings, v string) { s.PriceRefresh = v }), "You can also refresh them on the Tools page"),
		item("Price history", check("Load price history automatically", s.AutoPriceHistory, "price history", func(s *settings.Settings, v bool) { s.AutoPriceHistory = v }), "The first time, on desktop and web; phones always ask first"),
	)
	newCards := form(
		item("Condition", menu(conditionChoice, s.DefaultCondition, "default condition", func(s *settings.Settings, v string) { s.DefaultCondition = v }), "Preselected when adding a card"),
		item("Acquired date", menu(dateChoice, s.DefaultDate, "default date", func(s *settings.Settings, v string) { s.DefaultDate = v }), "Filled in when adding a card"),
	)
	advanced := form(
		item("Deleting", check("Ask before deleting cards", s.ConfirmDelete, "delete confirmation", func(s *settings.Settings, v bool) { s.ConfirmDelete = v }), ""),
		item("Log detail", menu(logChoice, s.LogLevel, "log level", func(s *settings.Settings, v string) { s.LogLevel = v }), "How much goes in the log file; --log-level overrides it at startup"),
	)

	// Where everything is stored.
	dataDir := filepath.Dir(ui.settings.Path())
	dataLabel := widget.NewLabel(dataDir)
	dataLabel.Wrapping = fyne.TextWrapBreak
	dataLabel.Selectable = true
	data := container.NewVBox(caption("Your collections, price history and settings are kept in this folder. Back it up to keep them safe."), dataLabel)
	if ui.settings.Path() == "" {
		dataLabel.SetText("(not saved: running without a data folder)")
	} else if !isMobile() {
		open := widget.NewButtonWithIcon("Open data folder", theme.FolderOpenIcon(), func() {
			u := &url.URL{Scheme: "file", Path: dataDir}
			if err := fyne.CurrentApp().OpenURL(u); err != nil {
				slog.Warn("could not open data folder", "err", err)
				dialog.ShowError(err, ui.window)
			}
		})
		data.Add(container.NewHBox(open))
	}

	reset := widget.NewButtonWithIcon("Restore defaults", theme.ContentUndoIcon(), func() {
		dialog.ShowConfirm("Restore defaults?", "Put every setting back to how it was when the app was installed? Your collection isn't affected.", func(ok bool) {
			if !ok {
				return
			}
			if err := ui.settings.Set(settings.Defaults()); err != nil {
				dialog.ShowError(err, ui.window)
				return
			}
			ui.statusLabel.SetText("Settings restored to their defaults")
			ui.showSettings()
		}, ui.window)
	})

	content := container.NewVBox(
		container.NewPadded(pageTitle("Settings")),
		caption("Changes are saved right away and apply to every game."),
		sectionTitle("Appearance"), appearance,
		sectionTitle("Collection"), collectionForm,
		sectionTitle("Prices"), prices,
		sectionTitle("Adding Cards"), newCards,
		sectionTitle("Advanced"), advanced,
		sectionTitle("Data"), data,
		sectionTitle("About"), widget.NewLabel(version.String("TCG Binder")),
		widget.NewSeparator(), container.NewHBox(reset),
	)
	ui.setRightPane(container.NewVScroll(container.NewPadded(content)))
}
