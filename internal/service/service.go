// Package service is the app's logic for one game's collection, shared by the
// desktop UI and the web server: opening its files, importing, refreshing prices,
// price history and the daily sync.
package service

import (
	"bytes"
	"errors"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"sync"
	"time"

	"github.com/jrhorner1/tcgbinder/internal/collection"
	"github.com/jrhorner1/tcgbinder/internal/datadir"
	"github.com/jrhorner1/tcgbinder/internal/game"
	"github.com/jrhorner1/tcgbinder/internal/settings"
)

// PriceCheckInterval is how often running apps check whether prices need refreshing.
// Prices refresh at most once a day, when the stored ones are from an earlier day.
const PriceCheckInterval = time.Hour

// AppFileFormat names collection files written by this app in import reports.
const AppFileFormat = "TCG Binder collection file"

// Game is one game with its open collection and price history.
type Game struct {
	Game   game.Game
	Store  *collection.Store
	Prices *collection.PriceHistory
	// Settings, if set, returns the app settings (price refresh frequency, automatic
	// price history).
	Settings func() settings.Settings

	mu   sync.Mutex
	sets map[string]collection.SetInfo
}

// Open prepares g's folder inside root and opens its collection and price history.
func Open(root string, g game.Game) (*Game, error) {
	dir, err := datadir.GameDir(root, g.ID())
	if err != nil {
		return nil, err
	}
	path := filepath.Join(dir, datadir.CollectionFile)
	if err := g.Open(dir); err != nil {
		return nil, err
	}
	store, err := collection.OpenStore(path, g.ID())
	if err != nil {
		return nil, err
	}
	slog.Info("collection opened", "game", g.ID(), "path", path, "cards", len(store.GetCollection()))
	return FromStore(g, store), nil
}

// FromStore wraps an open store, loading the price history kept next to it.
func FromStore(g game.Game, store *collection.Store) *Game {
	path := filepath.Join(filepath.Dir(store.FilePath()), "price-history.json")
	prices, err := collection.OpenPriceHistory(path)
	if err != nil {
		slog.Warn("price history", "game", g.ID(), "err", err)
	}
	if prices == nil {
		// The file exists but couldn't be read or set aside: keep the history in memory
		// for this session and leave the file untouched.
		prices, _ = collection.OpenPriceHistory(filepath.Join(os.TempDir(), "tcgbinder-"+g.ID()+"-price-history.json"))
	}
	return &Game{Game: g, Store: store, Prices: prices}
}

func today() string {
	y, m, d := time.Now().Date()
	return time.Date(y, m, d, 0, 0, 0, 0, time.UTC).Format("2006-01-02")
}

// Sets returns set sizes for completion stats, loading them once.
func (s *Game) Sets() (map[string]collection.SetInfo, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.sets != nil {
		return s.sets, nil
	}
	sets, err := s.Game.Sets()
	if err != nil {
		return nil, err
	}
	s.sets = sets
	return sets, nil
}

// CachedSets returns set sizes if already loaded, else nil.
func (s *Game) CachedSets() map[string]collection.SetInfo {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.sets
}

// RecordToday saves today's value snapshot and current prices to the histories.
func (s *Game) RecordToday() error {
	if err := s.Store.RecordSnapshot(); err != nil {
		return err
	}
	s.Prices.RecordCurrent(s.Store.GetCollection(), today())
	return s.Prices.Save()
}

// RefreshPrices fetches current prices from the game's catalog, saves them and
// records today's prices. It returns the number of cards updated.
func (s *Game) RefreshPrices(progress func(done, total int)) (int, error) {
	updates, err := s.Game.Refresh(s.Store.IDs(), progress)
	if err != nil {
		return 0, err
	}
	n, err := s.Store.UpdateCatalog(updates, today())
	if err != nil {
		return n, err
	}
	s.Prices.RecordCurrent(s.Store.GetCollection(), today())
	return n, s.Prices.Save()
}

// PriceHistorySource reports whether the game offers price history.
func (s *Game) PriceHistorySource() (game.PriceHistorySource, bool) {
	src, ok := s.Game.(game.PriceHistorySource)
	return src, ok
}

// LoadPriceHistory merges the game's daily price history (keeping prices already recorded)
// and returns how many printings had history.
func (s *Game) LoadPriceHistory(status func(string)) (int, error) {
	src, ok := s.PriceHistorySource()
	if !ok {
		return 0, errors.New("this game has no price history source")
	}
	cards := s.Store.GetCollection()
	if len(cards) == 0 {
		return 0, nil
	}
	if status == nil {
		status = func(string) {}
	}
	prices, err := src.PriceHistory(cards, status)
	if err != nil {
		return 0, err
	}
	for id, byFinish := range prices {
		s.Prices.Merge(id, byFinish)
	}
	s.Prices.MarkHistoryLoaded(today())
	return len(prices), s.Prices.Save()
}

// settings returns the app settings that steer automatic price updates. Without them
// (tests, tools), prices refresh daily and price history loads automatically.
func (s *Game) settings() settings.Settings {
	if s.Settings == nil {
		return settings.Defaults()
	}
	return s.Settings()
}

// PricesDue reports whether automatic updates should refresh prices now: they are from
// an earlier day (daily), a week or more old (weekly), or never (manual).
func (s *Game) PricesDue() bool {
	switch s.settings().PriceRefresh {
	case "manual":
		return false
	case "weekly":
		return s.Store.PricesOlderThan(7)
	}
	return s.Store.PricesStale()
}

// AutoLoadHistoryDue reports whether to load price history without being asked: the
// game offers it, it was never loaded, there are cards, and the setting allows it.
func (s *Game) AutoLoadHistoryDue() bool {
	_, ok := s.PriceHistorySource()
	return ok && s.settings().AutoPriceHistory && s.Prices.HistoryLoadedOn() == "" && len(s.Store.IDs()) > 0
}

// DailySync is the startup routine: load price history the first time (when allowed),
// refresh prices when due, and record today's value.
func (s *Game) DailySync() {
	if s.AutoLoadHistoryDue() {
		if n, err := s.LoadPriceHistory(nil); err != nil {
			slog.Warn("loading price history failed", "game", s.Game.ID(), "err", err)
		} else {
			slog.Info("price history loaded", "game", s.Game.ID(), "printings", n)
		}
	}
	if s.PricesDue() {
		if n, err := s.RefreshPrices(nil); err != nil {
			slog.Warn("price refresh failed", "game", s.Game.ID(), "err", err)
		} else {
			slog.Info("prices refreshed", "game", s.Game.ID(), "cards", n)
		}
		return
	}
	if err := s.RecordToday(); err != nil {
		slog.Warn("could not record today's value", "game", s.Game.ID(), "err", err)
	}
}

// RunPriceSchedule refreshes prices whenever they are due, checking every
// PriceCheckInterval until stop is closed. Used by the web server.
func (s *Game) RunPriceSchedule(stop <-chan struct{}) {
	t := time.NewTicker(PriceCheckInterval)
	defer t.Stop()
	for {
		select {
		case <-stop:
			return
		case <-t.C:
			if s.PricesDue() {
				if n, err := s.RefreshPrices(nil); err != nil {
					slog.Warn("scheduled price refresh failed", "game", s.Game.ID(), "err", err)
				} else {
					slog.Info("prices refreshed", "game", s.Game.ID(), "cards", n)
				}
			}
		}
	}
}

// ImportResult summarizes an import.
type ImportResult struct {
	Report                        game.ImportReport
	Cards, NewCards, AlreadyOwned int
}

// Summary describes the result for people.
func (r ImportResult) Summary() string {
	var b bytes.Buffer
	fmt.Fprintf(&b, "Format: %s\n", r.Report.Format)
	if r.Report.Format == AppFileFormat {
		fmt.Fprintf(&b, "Cards read: %d\n", r.Report.Rows)
	} else {
		fmt.Fprintf(&b, "Rows read: %d\n", r.Report.Rows)
	}
	fmt.Fprintf(&b, "Cards imported: %d (%d new cards, %d added to cards you already own)\n", r.Cards, r.NewCards, r.AlreadyOwned)
	for i, n := range r.Report.Notes {
		if i == 0 {
			b.WriteString("\n")
		}
		b.WriteString(n + "\n")
	}
	return b.String()
}

// Import adds a file's cards: a CSV export from another app, or a collection file
// written by this app. Cards for a printing already owned add to its quantity.
func (s *Game) Import(data []byte, progress func(done, total int)) (ImportResult, error) {
	cards, report, err := ParseImport(s.Game, data, progress)
	res := ImportResult{Report: report}
	if err != nil {
		return res, err
	}
	for _, c := range cards {
		res.Cards += c.CopyCount()
	}
	res.NewCards, res.AlreadyOwned, err = s.Store.AddCards(cards)
	if err == nil {
		slog.Info("import complete", "game", s.Game.ID(), "format", report.Format, "cards", res.Cards, "new", res.NewCards, "merged", res.AlreadyOwned)
	}
	return res, err
}

// IsAppFile reports whether data looks like a JSON collection file rather than CSV.
func IsAppFile(data []byte) bool {
	trimmed := bytes.TrimLeft(bytes.TrimPrefix(data, []byte("\ufeff")), " \t\r\n")
	return len(trimmed) > 0 && trimmed[0] == '{'
}

// ParseImport turns a file into cards for g without saving them. App collection
// files already carry exact catalog IDs and prices, so they are used as-is; anything
// else goes to the game's CSV importer.
func ParseImport(g game.Game, data []byte, progress func(done, total int)) ([]collection.Card, game.ImportReport, error) {
	if !IsAppFile(data) {
		return g.Import(data, progress)
	}
	report := game.ImportReport{Format: AppFileFormat}
	fileGame, cards, err := collection.ReadExport(data)
	if err != nil {
		return nil, report, err
	}
	if fileGame != g.ID() {
		return nil, report, fmt.Errorf("this file is a %q collection; it can't be imported into %s", fileGame, g.Name())
	}
	report.Rows = len(cards)
	if len(cards) == 0 {
		return nil, report, errors.New("no cards found in this collection file")
	}
	return cards, report, nil
}
