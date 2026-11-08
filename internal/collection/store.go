// Package collection holds the game-neutral collection model: cards, the JSON file
// store (one per game), price history, filtering, and value/completion statistics.
package collection

import (
	"bufio"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"maps"
	"math"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"
)

// Store is one game's collection, persisted as a JSON file. Each game has its own.
type Store struct {
	mu       sync.RWMutex
	saveMu   sync.Mutex // serializes Save so an older state never lands last
	filePath string
	Game     string          `json:"game,omitempty"` // ID of the game this file belongs to
	Cards    map[string]Card `json:"cards"`
	History  []ValueSnapshot `json:"history,omitempty"` // daily totals for the value-over-time chart
}

// OpenStore opens (or creates) the collection file for game at path. A file that
// belongs to a different game is refused; a new file is claimed for this one.
func OpenStore(path, game string) (*Store, error) {
	s, err := NewStore(path)
	if err != nil {
		return nil, err
	}
	s.mu.Lock()
	owner := s.Game
	if owner == "" {
		s.Game = game
	}
	s.mu.Unlock()
	switch {
	case owner == "":
		return s, s.Save()
	case owner != game:
		return nil, fmt.Errorf("%s holds a %q collection, not %q", path, owner, game)
	}
	return s, nil
}

// FilePath is the absolute path of the collection file.
func (cs *Store) FilePath() string { return cs.filePath }

// NewCollectionStore initializes a store pointing to a local file path.
func NewStore(filename string) (*Store, error) {
	if filename == "" {
		filename = "collection.json"
	}

	// Resolve absolute path to ensure clarity across operating systems
	absPath, err := filepath.Abs(filename)
	if err != nil {
		return nil, fmt.Errorf("failed to resolve absolute path: %w", err)
	}

	store := &Store{
		filePath: absPath,
		Cards:    make(map[string]Card),
	}

	// Auto-load existing records if the file exists
	if err := store.Load(); err != nil {
		if errors.Is(err, os.ErrNotExist) {
			// Initialize an empty file structure if it's a fresh run
			if err := store.Save(); err != nil {
				return nil, fmt.Errorf("failed to initialize collection file: %w", err)
			}
		} else {
			return nil, fmt.Errorf("failed to load existing collection: %w", err)
		}
	}

	return store, nil
}

// Save flushes the current state of the collection to disk using an atomic write.
func (cs *Store) Save() error {
	cs.saveMu.Lock()
	defer cs.saveMu.Unlock()
	cs.mu.RLock()
	defer cs.mu.RUnlock()

	// Ensure the parent directory exists
	dir := filepath.Dir(cs.filePath)
	if err := os.MkdirAll(dir, 0755); err != nil {
		return fmt.Errorf("failed to create storage directories: %w", err)
	}

	// Use a temporary file to guarantee atomic saves and protect against corruptions during unexpected crashes
	tmpFile, err := os.CreateTemp(dir, "collection_*.tmp")
	if err != nil {
		return fmt.Errorf("failed to create atomic temp file: %w", err)
	}
	tmpName := tmpFile.Name()
	defer func() {
		_ = tmpFile.Close()
		_ = os.Remove(tmpName) // Safely clean up if rename fails
	}()

	// Encode with indentation for human-readable diffs (critical for open-source local-first apps),
	// buffered: the encoder makes many small writes.
	buf := bufio.NewWriterSize(tmpFile, 1<<16)
	encoder := json.NewEncoder(buf)
	encoder.SetIndent("", "  ")
	if err := encoder.Encode(cs); err != nil {
		return fmt.Errorf("failed to encode collection JSON: %w", err)
	}
	if err := buf.Flush(); err != nil {
		return fmt.Errorf("failed to write collection file: %w", err)
	}

	// Sync to physical media
	if err := tmpFile.Sync(); err != nil {
		return fmt.Errorf("failed to sync file contents to disk: %w", err)
	}
	if err := tmpFile.Close(); err != nil {
		return fmt.Errorf("failed to close temp file descriptor: %w", err)
	}

	// Replace the old file atomically
	if err := os.Rename(tmpName, cs.filePath); err != nil {
		return fmt.Errorf("failed to replace collection file safely: %w", err)
	}

	return nil
}

// Load populates the in-memory map from the local storage file.
func (cs *Store) Load() error {
	cs.mu.Lock()
	defer cs.mu.Unlock()

	file, err := os.Open(cs.filePath)
	if err != nil {
		return err // Let caller distinguish os.ErrNotExist
	}
	defer file.Close()

	// Handle empty file states smoothly
	stat, err := file.Stat()
	if err != nil {
		return err
	}
	if stat.Size() == 0 {
		cs.Cards = make(map[string]Card)
		return nil
	}

	var tempStore Store
	if err := json.NewDecoder(file).Decode(&tempStore); err != nil {
		if errors.Is(err, io.EOF) {
			cs.Cards = make(map[string]Card)
			return nil
		}
		return fmt.Errorf("malformed collection file: %w", err)
	}

	cs.History = tempStore.History
	cs.Game = tempStore.Game
	cs.Cards = tempStore.Cards
	if cs.Cards == nil {
		cs.Cards = make(map[string]Card)
	}
	return nil
}

// CardKey identifies a collection card: one per printing, with its copies grouped
// by finish and condition in Copies.
func CardKey(c Card) string { return c.ID }

// normalizeCard applies the canonical spellings: lowercase finishes and language,
// NM for a missing condition, and no empty purchases.
func normalizeCard(c *Card) {
	if c.Language == "" {
		c.Language = "en"
	}
	c.Language = strings.ToLower(c.Language)
	h := Copies{}
	for _, l := range c.PurchaseRows() {
		h.add(l.Finish, l.Condition, l.Purchase)
	}
	if len(h) == 0 {
		h = nil
	}
	c.Copies = h
}

// mergeCards adds incoming's purchases to an existing card for the same printing
// (identical purchases combine). Catalog data (image, prices) comes from whichever
// side is newer; fields only one side has, such as game attributes or rules text,
// are kept either way.
func mergeCards(existing, incoming Card) Card {
	older, merged := incoming, existing
	if incoming.PricesUpdated > existing.PricesUpdated {
		older, merged = existing, incoming
	}
	for k, v := range older.Attrs {
		if merged.Attr(k) == nil {
			merged.SetAttr(k, v)
		}
	}
	if merged.RulesText == "" {
		merged.TypeLine, merged.RulesText = older.TypeLine, older.RulesText
	}
	merged.Copies = existing.Copies.clone()
	if merged.Copies == nil {
		merged.Copies = Copies{}
	}
	for _, l := range incoming.PurchaseRows() {
		merged.Copies.add(l.Finish, l.Condition, l.Purchase)
	}
	return merged
}

// AddCards adds copies to the collection, merging into the card for the same
// printing when there is one, and saves once. It reports how many new cards were
// created and how many cards were added to existing ones.
func (cs *Store) AddCards(cards []Card) (created, merged int, err error) {
	cs.mu.Lock()
	for _, card := range cards {
		if card.ID == "" || card.CopyCount() <= 0 {
			continue
		}
		normalizeCard(&card)
		key := CardKey(card)
		if existing, ok := cs.Cards[key]; ok {
			cs.Cards[key] = mergeCards(existing, card)
			merged++
		} else {
			cs.Cards[key] = card
			created++
		}
	}
	cs.snapshotLocked()
	cs.mu.Unlock()
	return created, merged, cs.Save()
}

// UpsertCard saves card in place of the one for its printing. No copies removes it.
func (cs *Store) UpsertCard(card Card) error {
	_, _, err := cs.SaveCard(CardKey(card), card)
	return err
}

// SaveCard saves an edited card that was stored under oldKey (its copies replace
// the old ones). If the card moved to another printing that is already in the
// collection, its copies merge into that card. No copies left deletes the card.
func (cs *Store) SaveCard(oldKey string, card Card) (newKey string, merged bool, err error) {
	if card.ID == "" {
		return "", false, errors.New("cannot save a card without a catalog ID")
	}
	for _, l := range card.PurchaseRows() {
		if l.Quantity < 0 {
			return "", false, errors.New("quantity cannot be negative")
		}
	}
	normalizeCard(&card)
	newKey = CardKey(card)

	cs.mu.Lock()
	delete(cs.Cards, oldKey)
	if card.CopyCount() > 0 {
		if existing, ok := cs.Cards[newKey]; ok && newKey != oldKey {
			cs.Cards[newKey] = mergeCards(existing, card)
			merged = true
		} else {
			cs.Cards[newKey] = card
		}
	}
	cs.snapshotLocked()
	cs.mu.Unlock()
	return newKey, merged, cs.Save()
}

// RemoveCard drops the card with the given CardKey.
func (cs *Store) RemoveCard(key string) error {
	cs.mu.Lock()
	if _, exists := cs.Cards[key]; !exists {
		cs.mu.Unlock()
		return nil // Idempotent execution
	}
	delete(cs.Cards, key)
	cs.snapshotLocked()
	cs.mu.Unlock()

	return cs.Save()
}

// snapshotLocked records today's totals in History. Callers hold cs.mu for writing.
func (cs *Store) snapshotLocked() {
	snap := ValueSnapshot{Date: time.Now().Format("2006-01-02")}
	for _, c := range cs.Cards {
		snap.Cards += c.CopyCount()
		paid, _ := c.PricePaid()
		value, _ := c.MarketValue()
		snap.PricePaid += paid
		snap.MarketValue += value
	}
	snap.MarketValue = math.Round(snap.MarketValue*100) / 100
	snap.PricePaid = math.Round(snap.PricePaid*100) / 100
	cs.History = upsertSnapshot(cs.History, snap)
}

// RecordSnapshot saves today's totals to the value history (replacing any earlier
// snapshot from today), using the stored market prices.
func (cs *Store) RecordSnapshot() error {
	cs.mu.Lock()
	cs.snapshotLocked()
	cs.mu.Unlock()
	return cs.Save()
}

// ValueHistory returns a copy of the recorded daily snapshots, oldest first.
func (cs *Store) ValueHistory() []ValueSnapshot {
	cs.mu.RLock()
	defer cs.mu.RUnlock()
	return append([]ValueSnapshot(nil), cs.History...)
}

// PricesStale reports whether any card's market prices were last refreshed before today.
func (cs *Store) PricesStale() bool { return cs.PricesOlderThan(1) }

// PricesOlderThan reports whether any card's market prices are at least days old
// (1: from before today; 7: from a week ago or earlier).
func (cs *Store) PricesOlderThan(days int) bool {
	cutoff := time.Now().AddDate(0, 0, 1-days).Format("2006-01-02")
	cs.mu.RLock()
	defer cs.mu.RUnlock()
	for _, c := range cs.Cards {
		if c.ID != "" && c.PricesUpdated < cutoff {
			return true
		}
	}
	return false
}

// IDs returns the distinct printing IDs in the collection.
func (cs *Store) IDs() []string {
	cs.mu.RLock()
	defer cs.mu.RUnlock()
	seen := map[string]bool{}
	var out []string
	for _, c := range cs.Cards {
		if c.ID != "" && !seen[c.ID] {
			seen[c.ID] = true
			out = append(out, c.ID)
		}
	}
	return out
}

// UpdateCatalog applies fresh prices (and missing rules text) from the game's
// catalog, records today's value snapshot and saves. It returns the cards updated.
func (cs *Store) UpdateCatalog(updates map[string]CatalogUpdate, date string) (int, error) {
	updated := 0
	cs.mu.Lock()
	for key, c := range cs.Cards {
		u, ok := updates[c.ID]
		if !ok {
			continue
		}
		c.Market = maps.Clone(u.Market)
		c.PricesUpdated = date
		if c.RulesText == "" && u.RulesText != "" { // fill in text for cards saved without it
			c.TypeLine, c.RulesText = u.TypeLine, u.RulesText
		}
		cs.Cards[key] = c
		updated++
	}
	cs.snapshotLocked()
	cs.mu.Unlock()
	return updated, cs.Save()
}

// ReadExport parses a collection file written by this app (for importing a backup or
// another device's collection). It returns the file's game and its cards.
func ReadExport(data []byte) (game string, cards []Card, err error) {
	var f struct {
		Game  string          `json:"game"`
		Cards map[string]Card `json:"cards"`
	}
	if err := json.Unmarshal(data, &f); err != nil {
		return "", nil, fmt.Errorf("not a collection file: %w", err)
	}
	if f.Cards == nil {
		return "", nil, errors.New("not a collection file: no \"cards\" found")
	}
	for _, c := range f.Cards {
		if c.ID != "" && c.CopyCount() > 0 {
			cards = append(cards, c)
		}
	}
	return f.Game, cards, nil
}

// Get returns the card with the given CardKey.
func (cs *Store) Get(key string) (Card, bool) {
	cs.mu.RLock()
	defer cs.mu.RUnlock()
	c, ok := cs.Cards[key]
	return c, ok
}

// GetCollection returns a copied slice of the active collection safely isolating raw thread access.
func (cs *Store) GetCollection() []Card {
	cs.mu.RLock()
	defer cs.mu.RUnlock()

	list := make([]Card, 0, len(cs.Cards))
	for _, card := range cs.Cards {
		list = append(list, card)
	}
	return list
}
