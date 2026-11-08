package ui

import (
	"slices"
	"strings"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/driver/desktop"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"github.com/jrhorner1/tcgbinder/internal/game"
)

// tapRow makes any row tappable (to open a card), with a hover highlight on desktop.
type tapRow struct {
	widget.BaseWidget
	content fyne.CanvasObject
	onTap   func()
	hover   *canvas.Rectangle
}

func newTapRow(content fyne.CanvasObject, onTap func()) *tapRow {
	r := &tapRow{content: content, onTap: onTap, hover: canvas.NewRectangle(theme.Color(theme.ColorNameHover))}
	r.hover.CornerRadius = 4
	r.hover.Hide()
	r.ExtendBaseWidget(r)
	return r
}

func (r *tapRow) CreateRenderer() fyne.WidgetRenderer {
	return widget.NewSimpleRenderer(container.NewStack(r.hover, r.content))
}

func (r *tapRow) Tapped(*fyne.PointEvent) {
	if r.onTap != nil {
		r.onTap()
	}
}

func (r *tapRow) Cursor() desktop.Cursor { return desktop.PointerCursor }

func (r *tapRow) MouseIn(*desktop.MouseEvent) {
	r.hover.FillColor = theme.Color(theme.ColorNameHover)
	r.hover.Show()
	r.hover.Refresh()
}

func (r *tapRow) MouseMoved(*desktop.MouseEvent) {}

func (r *tapRow) MouseOut() {
	r.hover.Hide()
	r.hover.Refresh()
}

// finishPicker is a menu of the finishes a printing actually comes in, shown by their
// display names ("Normal", "Foil") and returning the stored IDs ("nonfoil", "foil").
// Put its Select (not the picker) in layouts: Fyne tracks rendering per widget.
type finishPicker struct {
	*widget.Select
	g   game.Game
	ids map[string]string // display name -> ID
}

func newFinishPicker(g game.Game) *finishPicker {
	return &finishPicker{Select: widget.NewSelect(nil, nil), g: g, ids: map[string]string{}}
}

// SetFinishes offers available (the printing's finishes) and selects current. The
// current finish is always offered, even if the catalog doesn't list it, so saved data
// is never silently changed.
func (f *finishPicker) SetFinishes(available []string, current string) {
	if len(available) == 0 {
		available = f.g.Finishes()[:1]
	}
	if current != "" && !slices.Contains(available, current) {
		available = append(append([]string{}, available...), current)
	}
	f.ids = map[string]string{}
	names := make([]string, 0, len(available))
	for _, id := range available {
		name := f.g.FinishName(id)
		f.ids[name] = id
		names = append(names, name)
	}
	f.Select.SetOptions(names)
	if current == "" || !slices.Contains(available, current) {
		current = available[0]
	}
	f.Select.SetSelected(f.g.FinishName(current))
}

// Finish is the selected finish's ID.
func (f *finishPicker) Finish() string { return f.ids[f.Select.Selected] }

// newDatePicker is a date field with a calendar, set to date (YYYY-MM-DD) if valid.
func newDatePicker(date string) *widget.DateEntry {
	e := widget.NewDateEntry()
	if t, err := time.Parse("2006-01-02", date); err == nil {
		e.SetDate(&t)
	}
	return e
}

// pickedDate is the picker's date as YYYY-MM-DD, or "" when empty or invalid.
func pickedDate(e *widget.DateEntry) string {
	if e.Date == nil || strings.TrimSpace(e.Text) == "" {
		return ""
	}
	return e.Date.Format("2006-01-02")
}

// purchaseColumns lays out the card editor's purchase table: finish, condition,
// quantity, price paid and date, sharing the width by weight. Unlike a grid, it doesn't
// make every column as wide as the widest cell, so the table fits a narrow editor.
var purchaseColumns = weightedColumns{1.1, 0.75, 0.5, 0.85, 1.8}

type weightedColumns []float32

func (w weightedColumns) Layout(objects []fyne.CanvasObject, size fyne.Size) {
	pad := theme.Padding()
	var total float32
	for _, x := range w {
		total += x
	}
	avail := size.Width - pad*float32(len(w)-1)
	x := float32(0)
	for i, o := range objects {
		if i >= len(w) {
			break
		}
		cw := avail * w[i] / total
		o.Move(fyne.NewPos(x, 0))
		o.Resize(fyne.NewSize(cw, size.Height))
		x += cw + pad
	}
}

func (w weightedColumns) MinSize(objects []fyne.CanvasObject) fyne.Size {
	h := float32(0)
	for _, o := range objects {
		h = max(h, o.MinSize().Height)
	}
	// Each column may shrink to a small minimum; text is truncated rather than overflowing.
	return fyne.NewSize(float32(len(w))*60+theme.Padding()*float32(len(w)-1), h)
}
