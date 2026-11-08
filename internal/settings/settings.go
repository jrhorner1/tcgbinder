// Package settings holds the user's app settings (appearance, price updates, defaults
// for new cards, ...), shared by the desktop, phone and web apps. They are stored as
// settings.json in the data folder, so they move with the collection.
package settings

import (
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"log/slog"
	"os"
	"path/filepath"
	"slices"
	"sync"
)

// FileName is the settings file in the data folder.
const FileName = "settings.json"

// Choices for the settings with a fixed set of values.
var (
	Themes         = []string{"system", "light", "dark"}
	TextSizes      = []string{"small", "normal", "large", "larger"}
	CardOrders     = []string{"name", "newest", "oldest", "value"}
	PriceRefreshes = []string{"daily", "weekly", "manual"}
	ChartRanges    = []string{"30D", "90D", "1Y", "All"}
	Conditions     = []string{"NM", "LP", "MP", "HP", "DMG"}
	DefaultDates   = []string{"today", "none"}
	LogLevels      = []string{"debug", "info", "warn", "error"}
	textSizeScale  = map[string]float32{"small": 0.9, "normal": 1, "large": 1.15, "larger": 1.3}
)

// Settings are the user's choices. Fields missing from the file keep their defaults.
type Settings struct {
	// Appearance
	Theme      string `json:"theme"`       // system, light or dark
	TextSize   string `json:"text_size"`   // small, normal, large or larger
	ShowImages bool   `json:"show_images"` // download and show card images

	// Collection
	CardOrder  string `json:"card_order"`  // order of the collection's cards: name, by set (newest/oldest), value
	ChartRange string `json:"chart_range"` // value-over-time range shown first

	// Prices
	PriceRefresh     string `json:"price_refresh"`      // daily, weekly or manual
	AutoPriceHistory bool   `json:"auto_price_history"` // load price history automatically the first time (desktop and web)

	// New cards
	DefaultCondition string `json:"default_condition"` // condition preselected when adding a card
	DefaultDate      string `json:"default_date"`      // acquired date when adding a card: today or none

	// Advanced
	ConfirmDelete bool   `json:"confirm_delete"` // ask before deleting cards
	LogLevel      string `json:"log_level"`      // debug, info, warn or error
}

// Defaults are the settings before the user changes anything.
func Defaults() Settings {
	return Settings{
		Theme: "system", TextSize: "normal", ShowImages: true,
		CardOrder: "name", ChartRange: "90D",
		PriceRefresh: "daily", AutoPriceHistory: true,
		DefaultCondition: "NM", DefaultDate: "today",
		ConfirmDelete: true, LogLevel: "info",
	}
}

// TextScale is the factor applied to text and spacing for the text size.
func (s Settings) TextScale() float32 {
	if f, ok := textSizeScale[s.TextSize]; ok {
		return f
	}
	return 1
}

// normalize replaces values that aren't valid choices with the defaults.
func (s *Settings) normalize() {
	d := Defaults()
	fix := func(v *string, choices []string, def string) {
		if !slices.Contains(choices, *v) {
			*v = def
		}
	}
	fix(&s.Theme, Themes, d.Theme)
	fix(&s.TextSize, TextSizes, d.TextSize)
	fix(&s.CardOrder, CardOrders, d.CardOrder)
	fix(&s.ChartRange, ChartRanges, d.ChartRange)
	fix(&s.PriceRefresh, PriceRefreshes, d.PriceRefresh)
	fix(&s.DefaultCondition, Conditions, d.DefaultCondition)
	fix(&s.DefaultDate, DefaultDates, d.DefaultDate)
	fix(&s.LogLevel, LogLevels, d.LogLevel)
}

// Validate reports the first value that isn't one of its setting's choices.
func (s Settings) Validate() error {
	check := func(name, v string, choices []string) error {
		if !slices.Contains(choices, v) {
			return fmt.Errorf("%s must be one of %v, not %q", name, choices, v)
		}
		return nil
	}
	return errors.Join(
		check("theme", s.Theme, Themes),
		check("text_size", s.TextSize, TextSizes),
		check("card_order", s.CardOrder, CardOrders),
		check("chart_range", s.ChartRange, ChartRanges),
		check("price_refresh", s.PriceRefresh, PriceRefreshes),
		check("default_condition", s.DefaultCondition, Conditions),
		check("default_date", s.DefaultDate, DefaultDates),
		check("log_level", s.LogLevel, LogLevels),
	)
}

// Store keeps the settings and saves every change.
type Store struct {
	mu        sync.Mutex
	path      string
	s         Settings
	listeners []func(old, new Settings)
}

// Open reads the settings in dir (defaults if there is no file yet). A file that
// can't be read is set aside and the defaults are used, so the app always starts.
func Open(dir string) *Store {
	st := &Store{path: filepath.Join(dir, FileName), s: Defaults()}
	b, err := os.ReadFile(st.path)
	switch {
	case errors.Is(err, fs.ErrNotExist):
	case err != nil:
		slog.Warn("cannot read settings; using defaults", "path", st.path, "err", err)
	default:
		s := Defaults()
		if err := json.Unmarshal(b, &s); err != nil {
			bad := st.path + ".unreadable"
			slog.Warn("settings file is damaged; using defaults", "path", st.path, "moved_to", bad, "err", err)
			_ = os.Rename(st.path, bad)
		} else {
			s.normalize()
			st.s = s
		}
	}
	return st
}

// Memory returns a store that isn't saved anywhere (tests).
func Memory() *Store { return &Store{s: Defaults()} }

// Get returns the current settings.
func (st *Store) Get() Settings {
	st.mu.Lock()
	defer st.mu.Unlock()
	return st.s
}

// Path is the settings file ("" for a memory store).
func (st *Store) Path() string { return st.path }

// Set validates and saves new settings, then tells the listeners.
func (st *Store) Set(s Settings) error {
	if err := s.Validate(); err != nil {
		return err
	}
	st.mu.Lock()
	old := st.s
	if old == s {
		st.mu.Unlock()
		return nil
	}
	if st.path != "" {
		b, _ := json.MarshalIndent(s, "", "  ")
		tmp := st.path + ".tmp"
		if err := os.WriteFile(tmp, append(b, '\n'), 0o644); err != nil {
			st.mu.Unlock()
			return err
		}
		if err := os.Rename(tmp, st.path); err != nil {
			st.mu.Unlock()
			return err
		}
	}
	st.s = s
	listeners := slices.Clone(st.listeners)
	st.mu.Unlock()
	slog.Info("settings changed", "settings", fmt.Sprintf("%+v", s))
	for _, f := range listeners {
		f(old, s)
	}
	return nil
}

// Update changes some settings: Set(f(current)).
func (st *Store) Update(f func(*Settings)) error {
	s := st.Get()
	f(&s)
	return st.Set(s)
}

// OnChange calls f after every change, with the settings before and after.
func (st *Store) OnChange(f func(old, new Settings)) {
	st.mu.Lock()
	defer st.mu.Unlock()
	st.listeners = append(st.listeners, f)
}
