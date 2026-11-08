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

	"github.com/jrhorner1/mtgmanager/internal/collection"
	"github.com/jrhorner1/mtgmanager/internal/datadir"
	"github.com/jrhorner1/mtgmanager/internal/game"
)

// AppFileFormat names collection files written by this app in import reports.
const AppFileFormat = "MTG Manager collection file"

// Game is one game with its open collection and price history.
type Game struct {
	Game   game.Game
	Store  *collection.Store
	Prices *collection.PriceHistory

	mu   sync.Mutex
	sets map[string]collection.SetInfo
}

// OpenOptions controls Open.
type OpenOptions struct {
	// LegacyFiles moves files from before multi-game support (kept at the top of the
	// data folder) into this game's folder. Only the game that existed then sets it.
	LegacyFiles bool
	// LegacyCWD also copies a collection.json from the working directory, where the
	// earliest versions kept it (desktop only).
	LegacyCWD bool
}

// Open prepares g's folder inside root, opens its collection (upgrading entries
// from older versions) and its price history.
func Open(root string, g game.Game, opt OpenOptions) (*Game, error) {
	dir, err := datadir.GameDir(root, g.ID())
	if err != nil {
		return nil, err
	}
	path := filepath.Join(dir, datadir.CollectionFile)
	if opt.LegacyFiles {
		moved, err := datadir.MoveLegacyFiles(root, dir)
		if err != nil {
			return nil, err
		}
		if len(moved) > 0 {
			slog.Info("moved files into game folder", "files", moved, "dir", dir)
		}
	}
	if opt.LegacyCWD {
		copyLegacyCWD(path)
	}
	if err := g.Open(dir); err != nil {
		return nil, err
	}
	store, err := collection.OpenStore(path, g.ID())
	if err != nil {
		return nil, err
	}
	if n, err := store.Transform(g.Upgrade); err != nil {
		return nil, err
	} else if n > 0 {
		slog.Info("upgraded entries", "game", g.ID(), "entries", n)
	}
	slog.Info("collection opened", "game", g.ID(), "path", path, "entries", len(store.GetCollection()))
	return FromStore(g, store), nil
}

// FromStore wraps an open store, loading the price history kept next to it.
func FromStore(g game.Game, store *collection.Store) *Game {
	path := filepath.Join(filepath.Dir(store.FilePath()), "price-history.json")
	prices, err := collection.LoadPriceHistory(path)
	if err != nil {
		slog.Warn("price history", "game", g.ID(), "err", err)
	}
	if prices == nil {
		// The file exists but couldn't be read or set aside: keep the history in memory
		// for this session and leave the file untouched.
		prices, _ = collection.LoadPriceHistory(filepath.Join(os.TempDir(), "mtgmanager-"+g.ID()+"-price-history.json"))
	}
	return &Game{Game: g, Store: store, Prices: prices}
}

// copyLegacyCWD copies a collection.json from the working directory into dest the
// first time; the original is left in place.
func copyLegacyCWD(dest string) {
	if _, err := os.Stat(dest); err == nil {
		return
	}
	src, err := filepath.Abs(datadir.CollectionFile)
	if err != nil || src == dest {
		return
	}
	data, err := os.ReadFile(src)
	if err != nil {
		return // nothing to migrate
	}
	if err := os.WriteFile(dest, data, 0o600); err != nil {
		slog.Warn("could not copy legacy collection", "from", src, "err", err)
		return
	}
	slog.Info("copied legacy collection", "from", src, "to", dest)
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
// records today's prices. It returns the number of entries updated.
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

// CanBackfill reports whether the game offers price history.
func (s *Game) CanBackfill() (game.PriceHistorySource, bool) {
	src, ok := s.Game.(game.PriceHistorySource)
	return src, ok
}

// Backfill merges the game's daily price history (keeping prices already recorded)
// and returns how many printings had history.
func (s *Game) Backfill(status func(string)) (int, error) {
	src, ok := s.CanBackfill()
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
	s.Prices.MarkBackfilled(today())
	return len(prices), s.Prices.Save()
}

// DailySync is the startup routine: load price history the first time (when
// autoBackfill is set), refresh prices once a day, and record today's value.
func (s *Game) DailySync(autoBackfill bool) {
	if _, ok := s.CanBackfill(); ok && autoBackfill && s.Prices.BackfilledOn() == "" && len(s.Store.IDs()) > 0 {
		if n, err := s.Backfill(nil); err != nil {
			slog.Warn("price history backfill failed", "game", s.Game.ID(), "err", err)
		} else {
			slog.Info("price history loaded", "game", s.Game.ID(), "printings", n)
		}
	}
	if s.Store.PricesStale() {
		if n, err := s.RefreshPrices(nil); err != nil {
			slog.Warn("price refresh failed", "game", s.Game.ID(), "err", err)
		} else {
			slog.Info("prices refreshed", "game", s.Game.ID(), "entries", n)
		}
		return
	}
	if err := s.RecordToday(); err != nil {
		slog.Warn("could not record today's value", "game", s.Game.ID(), "err", err)
	}
}

// ImportResult summarizes an import.
type ImportResult struct {
	Report                  game.ImportReport
	Copies, Created, Merged int
}

// Summary describes the result for people.
func (r ImportResult) Summary() string {
	var b bytes.Buffer
	fmt.Fprintf(&b, "Format: %s\n", r.Report.Format)
	if r.Report.Format == AppFileFormat {
		fmt.Fprintf(&b, "Entries read: %d\n", r.Report.Rows)
	} else {
		fmt.Fprintf(&b, "Rows read: %d\n", r.Report.Rows)
	}
	fmt.Fprintf(&b, "Cards imported: %d (%d new entries, %d added to existing quantities)\n", r.Copies, r.Created, r.Merged)
	for i, n := range r.Report.Notes {
		if i == 0 {
			b.WriteString("\n")
		}
		b.WriteString(n + "\n")
	}
	return b.String()
}

// Import adds a file's cards: a CSV export from another app, or a collection file
// written by this app. Entries for a printing already owned add to its quantity.
func (s *Game) Import(data []byte, progress func(done, total int)) (ImportResult, error) {
	cards, report, err := ParseImport(s.Game, data, progress)
	res := ImportResult{Report: report}
	if err != nil {
		return res, err
	}
	for _, c := range cards {
		res.Copies += c.Quantity
	}
	res.Created, res.Merged, err = s.Store.AddCards(cards)
	if err == nil {
		slog.Info("import complete", "game", s.Game.ID(), "format", report.Format, "cards", res.Copies, "new", res.Created, "merged", res.Merged)
	}
	return res, err
}

// IsAppFile reports whether data looks like a JSON collection file rather than CSV.
func IsAppFile(data []byte) bool {
	trimmed := bytes.TrimLeft(bytes.TrimPrefix(data, []byte("\ufeff")), " \t\r\n")
	return len(trimmed) > 0 && trimmed[0] == '{'
}

// ParseImport turns a file into entries for g without saving them. App collection
// files already carry exact catalog IDs and prices, so they are used as-is (after
// upgrading entries from older versions); anything else goes to the game's CSV
// importer.
func ParseImport(g game.Game, data []byte, progress func(done, total int)) ([]collection.Card, game.ImportReport, error) {
	if !IsAppFile(data) {
		return g.Import(data, progress)
	}
	report := game.ImportReport{Format: AppFileFormat}
	fileGame, cards, err := collection.ReadExport(data)
	if err != nil {
		return nil, report, err
	}
	if fileGame != "" && fileGame != g.ID() {
		return nil, report, fmt.Errorf("this file is a %q collection; it can't be imported into %s", fileGame, g.Name())
	}
	for i := range cards {
		g.Upgrade(&cards[i])
	}
	report.Rows = len(cards)
	if len(cards) == 0 {
		return nil, report, errors.New("no cards found in this collection file")
	}
	return cards, report, nil
}
