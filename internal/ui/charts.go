package ui

import (
	"image/color"
	"math"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/driver/desktop"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"github.com/jrhorner1/tcgbinder/internal/collection"
)

var (
	colorMarket = color.NRGBA{R: 0x4c, G: 0x8d, B: 0xf6, A: 0xff} // blue
	colorPaid   = color.NRGBA{R: 0xe0, G: 0xa0, B: 0x30, A: 0xff} // amber
)

func chartText(s string, size float32, c color.Color) *canvas.Text {
	t := canvas.NewText(s, c)
	t.TextSize = size
	return t
}

// fitText shortens s with an ellipsis until it fits within width at the given size.
func fitText(s string, size, width float32) string {
	if fyne.MeasureText(s, size, fyne.TextStyle{}).Width <= width {
		return s
	}
	r := []rune(s)
	for len(r) > 1 {
		r = r[:len(r)-1]
		if t := string(r) + "…"; fyne.MeasureText(t, size, fyne.TextStyle{}).Width <= width {
			return t
		}
	}
	return "…"
}

func truncateRunes(s string, n int) string {
	r := []rune(s)
	if len(r) <= n {
		return s
	}
	return string(r[:n-1]) + "…"
}

type BarItem struct {
	Label     string
	Value     float64
	ValueText string
}

type BarChart struct {
	widget.BaseWidget
	Items []BarItem
	Max   float64 // scale maximum; 0 means the largest item value
	Color color.Color
	// OnTapped, if set, is called with the index of the row tapped.
	OnTapped func(i int)
}

// rowHeight matches the renderer's rows, so a tap can find its row.
func (b *BarChart) rowHeight() float32 {
	return chartText("Ag", theme.TextSize(), color.Black).MinSize().Height + theme.Padding()*2
}

// Tapped calls OnTapped with the row under the tap.
func (b *BarChart) Tapped(ev *fyne.PointEvent) {
	if i := int(ev.Position.Y / b.rowHeight()); b.OnTapped != nil && i >= 0 && i < len(b.Items) {
		b.OnTapped(i)
	}
}

// Cursor shows a pointer over rows that open something.
func (b *BarChart) Cursor() desktop.Cursor {
	if b.OnTapped != nil {
		return desktop.PointerCursor
	}
	return desktop.DefaultCursor
}

func NewBarChart(items []BarItem, max float64, c color.Color) *BarChart {
	b := &BarChart{Items: items, Max: max, Color: c}
	b.ExtendBaseWidget(b)
	return b
}

func (b *BarChart) CreateRenderer() fyne.WidgetRenderer {
	r := &barRenderer{chart: b}
	r.build()
	return r
}

type barRenderer struct {
	chart                  *BarChart
	labels, values         []*canvas.Text
	tracks, bars           []*canvas.Rectangle
	objects                []fyne.CanvasObject
	labelWidth, valueWidth float32
	rowHeight, textHeight  float32
}

func (r *barRenderer) build() {
	size := theme.TextSize()
	fg := theme.Color(theme.ColorNameForeground)
	track := theme.Color(theme.ColorNameInputBackground)
	r.labels, r.values, r.tracks, r.bars, r.objects = nil, nil, nil, nil, nil
	r.labelWidth, r.valueWidth = 0, 0
	for _, it := range r.chart.Items {
		l := chartText(truncateRunes(it.Label, 34), size, fg)
		v := chartText(it.ValueText, size, fg)
		t := canvas.NewRectangle(track)
		t.CornerRadius = 3
		bar := canvas.NewRectangle(r.chart.Color)
		bar.CornerRadius = 3
		r.labels, r.values, r.tracks, r.bars = append(r.labels, l), append(r.values, v), append(r.tracks, t), append(r.bars, bar)
		r.objects = append(r.objects, t, bar, l, v)
		r.labelWidth = max(r.labelWidth, l.MinSize().Width)
		r.valueWidth = max(r.valueWidth, v.MinSize().Width)
		r.textHeight = l.MinSize().Height
	}
	if r.textHeight == 0 {
		r.textHeight = chartText("Ag", size, fg).MinSize().Height
	}
	r.rowHeight = r.textHeight + theme.Padding()*2
}

func (r *barRenderer) scaleMax() float64 {
	m := r.chart.Max
	if m <= 0 {
		for _, it := range r.chart.Items {
			m = math.Max(m, it.Value)
		}
	}
	if m <= 0 {
		m = 1
	}
	return m
}

func (r *barRenderer) Layout(size fyne.Size) {
	pad := theme.Padding() * 2
	labelW := min(r.labelWidth, size.Width*0.42)
	barX := labelW + pad
	barMaxW := max(size.Width-barX-r.valueWidth-pad, 10)
	m := r.scaleMax()
	barH := r.textHeight * 0.7
	for i, it := range r.chart.Items {
		y := float32(i) * r.rowHeight
		r.labels[i].Text = fitText(truncateRunes(it.Label, 34), r.labels[i].TextSize, labelW)
		r.labels[i].Move(fyne.NewPos(0, y+(r.rowHeight-r.textHeight)/2))
		r.tracks[i].Move(fyne.NewPos(barX, y+(r.rowHeight-barH)/2))
		r.tracks[i].Resize(fyne.NewSize(barMaxW, barH))
		w := float32(math.Max(0, math.Min(it.Value/m, 1))) * barMaxW
		r.bars[i].Move(fyne.NewPos(barX, y+(r.rowHeight-barH)/2))
		r.bars[i].Resize(fyne.NewSize(w, barH))
		r.values[i].Move(fyne.NewPos(barX+barMaxW+pad, y+(r.rowHeight-r.textHeight)/2))
	}
}

func (r *barRenderer) MinSize() fyne.Size {
	return fyne.NewSize(r.labelWidth*0.6+r.valueWidth+120, float32(len(r.chart.Items))*r.rowHeight)
}

func (r *barRenderer) Refresh() {
	r.build()
	r.Layout(r.chart.Size())
	canvas.Refresh(r.chart)
}

func (r *barRenderer) Objects() []fyne.CanvasObject { return r.objects }

func (r *barRenderer) Destroy() {}

type Series struct {
	Name   string
	Color  color.Color
	Points []collection.TimePoint
	Step   bool // hold each value until the next point (for running totals)
}

type LineChart struct {
	widget.BaseWidget
	Series  []Series
	FormatY func(float64) string
	Empty   string    // shown when there are no points
	XMin    time.Time // optional fixed date range; zero means fit the data
	XMax    time.Time
}

func NewLineChart(series []Series, formatY func(float64) string) *LineChart {
	l := &LineChart{Series: series, FormatY: formatY}
	l.ExtendBaseWidget(l)
	return l
}

func (l *LineChart) CreateRenderer() fyne.WidgetRenderer {
	return &lineRenderer{chart: l}
}

type lineRenderer struct {
	chart   *LineChart
	objects []fyne.CanvasObject
}

func (r *lineRenderer) MinSize() fyne.Size { return fyne.NewSize(320, 240) }

func (r *lineRenderer) Refresh() {
	r.Layout(r.chart.Size())
	canvas.Refresh(r.chart)
}

func (r *lineRenderer) Objects() []fyne.CanvasObject { return r.objects }

func (r *lineRenderer) Destroy() {}

// Layout rebuilds every primitive for the current size; charts are small, so this is cheap.
func (r *lineRenderer) Layout(size fyne.Size) {
	r.objects = r.objects[:0]
	if size.Width < 50 || size.Height < 50 {
		return
	}
	fg := theme.Color(theme.ColorNameForeground)
	muted := theme.Color(theme.ColorNamePlaceHolder)
	grid := theme.Color(theme.ColorNameSeparator)
	ts := theme.CaptionTextSize()
	pad := theme.Padding()
	format := r.chart.FormatY
	if format == nil {
		format = func(v float64) string { return formatUSD(v) }
	}

	// Legend.
	x := float32(0)
	for _, s := range r.chart.Series {
		sw := canvas.NewRectangle(s.Color)
		sw.CornerRadius = 2
		sw.Resize(fyne.NewSize(12, 12))
		t := chartText(s.Name, ts, fg)
		th := t.MinSize().Height
		sw.Move(fyne.NewPos(x, (th-12)/2))
		t.Move(fyne.NewPos(x+16, 0))
		r.objects = append(r.objects, sw, t)
		x += 16 + t.MinSize().Width + pad*4
	}
	legendH := chartText("Ag", ts, fg).MinSize().Height + pad*2

	// Domains.
	var tMin, tMax time.Time
	yMax, n := 0.0, 0
	for _, s := range r.chart.Series {
		for _, p := range s.Points {
			if n == 0 || p.Date.Before(tMin) {
				tMin = p.Date
			}
			if n == 0 || p.Date.After(tMax) {
				tMax = p.Date
			}
			yMax = math.Max(yMax, p.Value)
			n++
		}
	}
	if n == 0 {
		msg := r.chart.Empty
		if msg == "" {
			msg = "No data yet"
		}
		t := chartText(msg, theme.TextSize(), muted)
		t.Move(fyne.NewPos((size.Width-t.MinSize().Width)/2, size.Height/2))
		r.objects = append(r.objects, t)
		return
	}
	if !r.chart.XMin.IsZero() {
		tMin = r.chart.XMin
	}
	if !r.chart.XMax.IsZero() {
		tMax = r.chart.XMax
	}
	if !tMax.After(tMin) {
		tMin, tMax = tMin.AddDate(0, 0, -15), tMax.AddDate(0, 0, 15)
	}
	dateFormat := "Jan 2006"
	if tMax.Sub(tMin) < 400*24*time.Hour {
		dateFormat = "Jan 2"
	}
	yMax = niceCeil(yMax * 1.05)

	// Plot area.
	yLabelW := chartText(format(yMax), ts, fg).MinSize().Width
	xLabelH := chartText("Ag", ts, fg).MinSize().Height
	left, top := yLabelW+pad*2, legendH
	plotW, plotH := size.Width-left-pad, size.Height-top-xLabelH-pad*2
	if plotW < 20 || plotH < 20 {
		return
	}
	px := func(t time.Time) float32 {
		return left + float32(t.Sub(tMin).Seconds()/tMax.Sub(tMin).Seconds())*plotW
	}
	py := func(v float64) float32 { return top + plotH - float32(v/yMax)*plotH }

	for i := 0; i <= 4; i++ {
		v := yMax * float64(i) / 4
		y := py(v)
		line := canvas.NewLine(grid)
		line.StrokeWidth = 1
		line.Position1, line.Position2 = fyne.NewPos(left, y), fyne.NewPos(left+plotW, y)
		lbl := chartText(format(v), ts, muted)
		lbl.Move(fyne.NewPos(left-pad-lbl.MinSize().Width, y-lbl.MinSize().Height/2))
		r.objects = append(r.objects, line, lbl)
	}
	for i, t := range []time.Time{tMin, tMin.Add(tMax.Sub(tMin) / 2), tMax} {
		lbl := chartText(t.Format(dateFormat), ts, muted)
		w := lbl.MinSize().Width
		xx := px(t) - w/2
		if i == 0 {
			xx = left
		} else if i == 2 {
			xx = left + plotW - w
		}
		lbl.Move(fyne.NewPos(xx, top+plotH+pad))
		r.objects = append(r.objects, lbl)
	}

	for _, s := range r.chart.Series {
		addSeg := func(p1, p2 fyne.Position) {
			seg := canvas.NewLine(s.Color)
			seg.StrokeWidth = 2
			seg.Position1, seg.Position2 = p1, p2
			r.objects = append(r.objects, seg)
		}
		for i := 1; i < len(s.Points); i++ {
			a, b := s.Points[i-1], s.Points[i]
			pa, pb := fyne.NewPos(px(a.Date), py(a.Value)), fyne.NewPos(px(b.Date), py(b.Value))
			if s.Step {
				corner := fyne.NewPos(pb.X, pa.Y)
				addSeg(pa, corner)
				addSeg(corner, pb)
			} else {
				addSeg(pa, pb)
			}
		}
		if len(s.Points) <= 40 { // dots help when there are few points (e.g. a single snapshot)
			for _, p := range s.Points {
				dot := canvas.NewCircle(s.Color)
				dot.Resize(fyne.NewSize(7, 7))
				dot.Move(fyne.NewPos(px(p.Date)-3.5, py(p.Value)-3.5))
				r.objects = append(r.objects, dot)
			}
		}
	}
}

// niceCeil rounds up to 1, 2, 2.5 or 5 times a power of ten, for readable axis labels.
func niceCeil(v float64) float64 {
	if v <= 0 {
		return 1
	}
	p := math.Pow(10, math.Floor(math.Log10(v)))
	for _, m := range []float64{1, 2, 2.5, 5, 10} {
		if v <= m*p {
			return m * p
		}
	}
	return 10 * p
}
