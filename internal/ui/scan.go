package ui

import (
	"errors"
	"fmt"
	"io"
	"log/slog"
	"strconv"
	"strings"
	"sync"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/storage"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"github.com/jrhorner1/tcgbinder/internal/collection"
	"github.com/jrhorner1/tcgbinder/internal/game"
	"github.com/jrhorner1/tcgbinder/internal/imagecache"
	"github.com/jrhorner1/tcgbinder/internal/scan"
	"github.com/jrhorner1/tcgbinder/internal/scan/apple"
)

// Card scanning: the phone's camera (or a photo) is read with Apple's Vision
// framework, and the active game identifies the card from the text. Tests replace
// these to scan without a camera.
var (
	cameraScanAvailable = apple.ScanAvailable // iOS: live camera / photo library
	photoReadAvailable  = apple.ReadAvailable // macOS and iOS: read a photo file
	startCameraScan     = apple.Scan
	readPhoto           = apple.ReadImage
)

// scanner returns the active game's scanner, if it has one.
func (ui *App) scanner() (game.Scanner, bool) {
	s, ok := ui.g.game.(game.Scanner)
	return s, ok
}

// canScanCamera reports whether the Scan button (live camera) is offered.
func (ui *App) canScanCamera() bool {
	_, ok := ui.scanner()
	return ok && cameraScanAvailable()
}

// canScanPhoto reports whether a card can be identified from a photo file (desktop).
func (ui *App) canScanPhoto() bool {
	_, ok := ui.scanner()
	return ok && photoReadAvailable() && !isMobile()
}

// startScan opens the camera scanner (or, with photosOnly, the photo library). It
// captures once two frames in a row read the same complete card text.
func (ui *App) startScan(photosOnly bool) {
	sc, ok := ui.scanner()
	if !ok {
		return
	}
	var mu sync.Mutex
	last := ""
	judge := func(r scan.Reading) apple.Frame {
		key, complete := sc.ScanKey(r)
		mu.Lock()
		defer mu.Unlock()
		switch {
		case !complete:
			last = ""
			return apple.FrameNothing
		case key == last:
			return apple.FrameAccept
		default:
			last = key
			return apple.FramePartial
		}
	}
	startCameraScan(photosOnly, judge, func(r scan.Reading, err error) {
		runOnUI(func() {
			switch {
			case errors.Is(err, apple.ErrCancelled):
			case err != nil:
				dialog.ShowError(err, ui.window)
			default:
				ui.identify(r, true)
			}
		})
	})
}

// scanPhotoFile identifies a card from a photo chosen on disk (desktop).
func (ui *App) scanPhotoFile() {
	open := dialog.NewFileOpen(func(rc fyne.URIReadCloser, err error) {
		if err != nil {
			dialog.ShowError(err, ui.window)
			return
		}
		if rc == nil {
			return // cancelled
		}
		defer rc.Close()
		data, err := io.ReadAll(rc)
		if err != nil {
			dialog.ShowError(err, ui.window)
			return
		}
		ui.statusLabel.SetText("Reading the photo…")
		go func() {
			r, err := readPhoto(data)
			runOnUI(func() {
				if err != nil {
					ui.statusLabel.SetText("")
					dialog.ShowError(fmt.Errorf("could not read the photo: %w", err), ui.window)
					return
				}
				ui.identify(r, false)
			})
		}()
	}, ui.window)
	open.SetFilter(storage.NewExtensionFileFilter([]string{".jpg", ".jpeg", ".png", ".heic", ".heif", ".tif", ".tiff"}))
	open.Show()
}

// identify looks up the card a reading describes and shows the result. fromCamera
// offers "Add & scan next".
func (ui *App) identify(r scan.Reading, fromCamera bool) {
	sc, ok := ui.scanner()
	if !ok {
		return
	}
	gs := ui.g
	slog.Debug("scanned", "game", gs.game.ID(), "card_found", r.CardFound, "text", r.Text())
	ui.page = "scan"
	if !isMobile() {
		ui.tree.UnselectAll() // the page isn't in the tree
	}
	ui.setRightPane(container.NewCenter(container.NewVBox(widget.NewProgressBarInfinite(), widget.NewLabel("Looking up the card…"))))
	ui.showDetail("Scanned Card")
	ui.statusLabel.SetText("Looking up the card…")
	go func() {
		res, err := sc.Identify(r)
		runOnUI(func() {
			if ui.page != "scan" || ui.g != gs {
				return // the user moved on
			}
			if err != nil {
				ui.statusLabel.SetText("Lookup failed")
				ui.showScanResult(game.ScanResult{Read: "", Note: "The card couldn't be looked up: " + err.Error()}, 0, fromCamera)
				return
			}
			ui.statusLabel.SetText("Scanned: " + res.Read)
			ui.showScanResult(res, 0, fromCamera)
		})
	}()
}

// showScanResult shows the scanned card (candidate i) with the purchase details to
// add it, the other printings it could be, and ways to scan again.
func (ui *App) showScanResult(res game.ScanResult, i int, fromCamera bool) {
	ui.page = "scan"
	gs := ui.g
	content := container.NewVBox(container.NewPadded(pageTitle("Scanned Card")))
	if res.Read != "" {
		content.Add(caption("Read from the card: " + res.Read))
	}
	if res.Note != "" {
		note := widget.NewLabel(res.Note)
		note.Wrapping = fyne.TextWrapWord
		note.Importance = widget.WarningImportance
		content.Add(note)
	}

	again := widget.NewButtonWithIcon("Scan again", theme.ViewRefreshIcon(), func() {
		if fromCamera {
			ui.startScan(false)
		} else {
			ui.scanPhotoFile()
		}
	})
	search := widget.NewButtonWithIcon("Search by name…", theme.SearchIcon(), func() { ui.showAddDialogFor(res.Name, "") })
	if len(res.Candidates) == 0 {
		content.Add(container.NewHBox(again, search))
		ui.setRightPane(container.NewVScroll(container.NewPadded(content)))
		return
	}

	card := res.Candidates[i]
	s := ui.settings.Get()

	// The card: image and what it is.
	img := canvas.NewImageFromResource(theme.DownloadIcon())
	img.FillMode = canvas.ImageFillContain
	img.SetMinSize(fyne.NewSize(200, 279))
	if !s.ShowImages || card.ImageURI == "" {
		img.Resource = theme.FileImageIcon()
		img.SetMinSize(fyne.NewSize(100, 100))
	} else {
		go func() {
			b, err := imagecache.Fetch(card.ImageURI)
			runOnUI(func() {
				if err != nil || len(b) == 0 {
					img.Resource = theme.BrokenImageIcon()
				} else {
					img.Resource = fyne.NewStaticResource(card.ID+".jpg", b)
				}
				img.Refresh()
			})
		}()
	}
	name := widget.NewLabelWithStyle(card.Name, fyne.TextAlignLeading, fyne.TextStyle{Bold: true})
	name.Wrapping = fyne.TextWrapWord
	where := widget.NewLabel(fmt.Sprintf("%s\n%s · %s", card.SetName, setNumber(strings.ToUpper(card.Set), card.CollectorNumber), titleCase(card.Rarity)))
	where.Wrapping = fyne.TextWrapWord
	var prices []string
	for _, f := range card.Finishes {
		if p, ok := collection.MarketPrice(card, f); ok {
			prices = append(prices, fmt.Sprintf("%s %s", formatUSD(p), gs.game.FinishName(f)))
		}
	}
	info := container.NewVBox(name, where)
	if len(prices) > 0 {
		info.Add(widget.NewLabel(strings.Join(prices, " · ")))
	}
	for _, owned := range gs.cards {
		if owned.ID == card.ID {
			info.Add(caption("You own " + copiesSummary(owned, gs.game) + " already; adding puts these with them."))
		}
	}
	if res.Exact {
		info.Add(caption("Matched by set and collector number."))
	}

	// The purchase to add.
	finish := newFinishPicker(gs.game)
	finish.SetFinishes(card.Finishes, "")
	cond := widget.NewSelect(conditionChoice.names, nil)
	cond.SetSelected(conditionChoice.name(s.DefaultCondition))
	qty := widget.NewEntry()
	qty.SetText("1")
	price := widget.NewEntry()
	price.SetPlaceHolder("price paid (each)")
	date := ""
	if s.DefaultDate == "today" {
		date = time.Now().Format("2006-01-02")
	}
	dateEntry := newDatePicker(date)
	form := widget.NewForm(
		widget.NewFormItem("Finish", finish.Select),
		widget.NewFormItem("Condition", cond),
		widget.NewFormItem("Quantity", qty),
		widget.NewFormItem("Price paid", price),
		widget.NewFormItem("Acquired", dateEntry),
	)
	if isMobile() {
		form.Orientation = widget.Vertical
	}

	add := func(next bool) {
		n, err := strconv.Atoi(strings.TrimSpace(qty.Text))
		if err != nil || n <= 0 {
			dialog.ShowError(errors.New("quantity must be a whole number of at least 1"), ui.window)
			return
		}
		paid := 0.0
		if p := strings.TrimPrefix(strings.TrimSpace(price.Text), "$"); p != "" {
			if paid, err = strconv.ParseFloat(p, 64); err != nil || paid < 0 {
				dialog.ShowError(errors.New("price paid must be a number, e.g. 2.50"), ui.window)
				return
			}
		}
		newCard := card
		newCard.Copies = nil
		newCard.AddPurchase(finish.Finish(), collection.Condition(conditionChoice.value(cond.Selected)),
			collection.Purchase{Quantity: n, PricePaid: paid, AcquiredDate: pickedDate(dateEntry)})
		if _, _, err := gs.store.AddCards([]collection.Card{newCard}); err != nil {
			dialog.ShowError(fmt.Errorf("save failed: %w", err), ui.window)
			return
		}
		ui.RefreshDataIndex()
		ui.statusLabel.SetText(fmt.Sprintf("Added %d× %s (%s)", n, card.Name, setNumber(strings.ToUpper(card.Set), card.CollectorNumber)))
		if next {
			ui.startScan(false)
			return
		}
		ui.selectCardNode(card)
	}
	addButton := widget.NewButtonWithIcon("Add to collection", theme.ContentAddIcon(), func() { add(false) })
	addButton.Importance = widget.HighImportance
	buttons := container.NewHBox(addButton)
	if fromCamera {
		addNext := widget.NewButtonWithIcon("Add & scan next", theme.MediaRecordIcon(), func() { add(true) })
		addNext.Importance = widget.HighImportance
		buttons.Add(addNext)
	}
	more := container.NewHBox(again, search)

	var top fyne.CanvasObject
	if isMobile() {
		top = container.NewVBox(container.NewCenter(img), info)
	} else {
		top = container.NewBorder(nil, nil, img, nil, info)
	}
	content.Add(top)
	content.Add(sectionTitle("Add to Your Collection"))
	content.Add(form)
	if isMobile() {
		content.Add(container.NewGridWithColumns(1, buttons.Objects...))
		content.Add(container.NewGridWithColumns(2, more.Objects...))
	} else {
		content.Add(buttons)
		content.Add(more)
	}

	// The other printings it could be (old cards don't print their set).
	if len(res.Candidates) > 1 {
		title := "Other Printings"
		if !res.Exact {
			title = "Not this printing? Pick yours"
		}
		content.Add(sectionTitle(title))
		const limit = 40
		for j, c := range res.Candidates {
			if j == limit {
				content.Add(caption(fmt.Sprintf("%d more; use Search by name to see them all.", len(res.Candidates)-limit)))
				break
			}
			j, c := j, c
			label := widget.NewLabel(c.SetName)
			label.Truncation = fyne.TextTruncateEllipsis
			if j == i {
				label.TextStyle = fyne.TextStyle{Bold: true}
			}
			year := ""
			if len(c.ReleasedAt) >= 4 {
				year = c.ReleasedAt[:4]
			}
			detail := widget.NewLabel(strings.TrimSuffix(setNumber(strings.ToUpper(c.Set), c.CollectorNumber)+" · "+year, " · "))
			content.Add(newTapRow(container.NewBorder(nil, nil, nil, detail, label), func() {
				ui.showScanResult(res, j, fromCamera)
			}))
		}
	}
	ui.setRightPane(container.NewVScroll(container.NewPadded(content)))
}
