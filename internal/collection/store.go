// Package collection holds the game-neutral collection model: entries, the JSON file
// store (one per game), price history, filtering, and value/completion statistics.
package collection

import (
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
	filePath string
	Game     string          `json:"game,omitempty"` // ID of the game this file belongs to
	Cards    map[string]Card `json:"cards"`
	History  []ValueSnapshot `json:"history,omitempty"` // daily totals for the value-over-time chart
}

// OpenStore opens (or creates) the collection file for game at path. A file that
// belongs to a different game is refused; a file without a game (from versions
// before multi-game support) is claimed for this one.
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

	// Encode with indentation for human-readable diffs (critical for open-source local-first apps)
	encoder := json.NewEncoder(tmpFile)
	encoder.SetIndent("", "  ")
	if err := encoder.Encode(cs); err != nil {
		return fmt.Errorf("failed to encode collection JSON: %w", err)
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

	// Re-key every entry so older files (keyed by Scryfall ID alone, or with
	// inconsistent finish spellings) collapse into the current scheme.
	cs.Cards = make(map[string]Card, len(tempStore.Cards))
	for _, card := range tempStore.Cards {
		normalizeEntry(&card)
		key := EntryKey(card)
		if existing, ok := cs.Cards[key]; ok {
			cs.Cards[key] = mergeEntries(existing, card)
		} else {
			cs.Cards[key] = card
		}
	}

	return nil
}

// EntryKey identifies a collection entry: copies of the same printing in the same
// finish, condition and language are one entry with a quantity.
func EntryKey(c Card) string {
	return strings.Join([]string{c.ID, c.Finish, string(c.Condition), c.Language}, "|")
}

// normalizeEntry applies the canonical spellings EntryKey depends on.
func normalizeEntry(c *Card) {
	c.Finish = strings.ToLower(strings.TrimSpace(c.Finish))
	if c.Condition == "" {
		c.Condition = ConditionNearMint
	}
	if c.Language == "" {
		c.Language = "en"
	}
	c.Language = strings.ToLower(c.Language)
}

// mergeEntries adds incoming copies to an existing entry. Quantities sum, the purchase
// price becomes the quantity-weighted average of the known prices, the earliest
// acquisition date is kept, and catalog data (image, prices) is taken from whichever
// side has the newer Scryfall data.
func mergeEntries(existing, incoming Card) Card {
	merged := existing
	if incoming.PricesUpdated > existing.PricesUpdated {
		merged = incoming
	}
	merged.Quantity = existing.Quantity + incoming.Quantity

	switch {
	case existing.PurchasePrice > 0 && incoming.PurchasePrice > 0:
		total := existing.PurchasePrice*float64(existing.Quantity) + incoming.PurchasePrice*float64(incoming.Quantity)
		merged.PurchasePrice = math.Round(total/float64(merged.Quantity)*100) / 100
	case existing.PurchasePrice > 0:
		merged.PurchasePrice = existing.PurchasePrice
	default:
		merged.PurchasePrice = incoming.PurchasePrice
	}

	merged.AcquiredDate = existing.AcquiredDate
	if merged.AcquiredDate == "" || (incoming.AcquiredDate != "" && incoming.AcquiredDate < merged.AcquiredDate) {
		merged.AcquiredDate = incoming.AcquiredDate
	}
	return merged
}

// AddCards adds copies to the collection, merging into existing entries with the same
// EntryKey, and saves once. It reports how many entries were created vs. merged into.
func (cs *Store) AddCards(cards []Card) (created, merged int, err error) {
	cs.mu.Lock()
	for _, card := range cards {
		if card.ID == "" || card.Quantity <= 0 {
			continue
		}
		normalizeEntry(&card)
		key := EntryKey(card)
		if existing, ok := cs.Cards[key]; ok {
			cs.Cards[key] = mergeEntries(existing, card)
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

// UpsertCard replaces the entry for card (by EntryKey) and saves. A quantity of zero removes it.
func (cs *Store) UpsertCard(card Card) error {
	if card.ID == "" {
		return errors.New("cannot track card with empty identifier ID")
	}
	if card.Quantity < 0 {
		return errors.New("card quantity cannot be negative")
	}
	normalizeEntry(&card)
	key := EntryKey(card)

	cs.mu.Lock()
	if card.Quantity == 0 {
		delete(cs.Cards, key)
	} else {
		cs.Cards[key] = card
	}
	cs.snapshotLocked()
	cs.mu.Unlock()

	return cs.Save()
}

// UpdateEntry saves an edited entry that was stored under oldKey. Editing the finish,
// condition or language changes the entry's key; if another entry already has the new
// key, the edited copies are merged into it. A quantity of zero deletes the entry.
func (cs *Store) UpdateEntry(oldKey string, card Card) (newKey string, merged bool, err error) {
	if card.ID == "" {
		return "", false, errors.New("cannot save a card without a Scryfall ID")
	}
	if card.Quantity < 0 {
		return "", false, errors.New("quantity cannot be negative")
	}
	normalizeEntry(&card)
	newKey = EntryKey(card)

	cs.mu.Lock()
	delete(cs.Cards, oldKey)
	if card.Quantity > 0 {
		if existing, ok := cs.Cards[newKey]; ok && newKey != oldKey {
			cs.Cards[newKey] = mergeEntries(existing, card)
			merged = true
		} else {
			cs.Cards[newKey] = card
		}
	}
	cs.snapshotLocked()
	cs.mu.Unlock()
	return newKey, merged, cs.Save()
}

// SetCatalogText fills in rules text and type line for every entry of a printing that
// lacks them (entries imported before these fields were stored). It does not save;
// the next write persists it.
func (cs *Store) SetCatalogText(id, typeLine, text string) {
	cs.mu.Lock()
	defer cs.mu.Unlock()
	for key, c := range cs.Cards {
		if c.ID == id && (c.Text == "" || c.TypeLine == "") {
			c.TypeLine, c.Text = typeLine, text
			cs.Cards[key] = c
		}
	}
}

// RemoveCard drops the entry with the given EntryKey.
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
		snap.Copies += c.Quantity
		snap.Cost += c.PurchasePrice * float64(c.Quantity)
		if p, ok := MarketPrice(c); ok {
			snap.Market += p * float64(c.Quantity)
		}
	}
	snap.Market = math.Round(snap.Market*100) / 100
	snap.Cost = math.Round(snap.Cost*100) / 100
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

// PricesStale reports whether any entry's market prices were last refreshed before today.
func (cs *Store) PricesStale() bool {
	today := time.Now().Format("2006-01-02")
	cs.mu.RLock()
	defer cs.mu.RUnlock()
	for _, c := range cs.Cards {
		if c.ID != "" && c.PricesUpdated < today {
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
// catalog, records today's value snapshot and saves. It returns the entries updated.
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
		if c.Text == "" && u.Text != "" { // fill in text for entries saved without it
			c.TypeLine, c.Text = u.TypeLine, u.Text
		}
		cs.Cards[key] = c
		updated++
	}
	cs.snapshotLocked()
	cs.mu.Unlock()
	return updated, cs.Save()
}

// Transform applies fn to every entry (fn returns true when it changed the card),
// re-keys changed entries, and saves if anything changed. Games use it for one-time
// upgrades of their stored data.
func (cs *Store) Transform(fn func(*Card) bool) (int, error) {
	changed := 0
	cs.mu.Lock()
	next := make(map[string]Card, len(cs.Cards))
	for _, c := range cs.Cards {
		if fn(&c) {
			changed++
			normalizeEntry(&c)
		}
		key := EntryKey(c)
		if existing, ok := next[key]; ok {
			next[key] = mergeEntries(existing, c)
		} else {
			next[key] = c
		}
	}
	if changed > 0 {
		cs.Cards = next
	}
	cs.mu.Unlock()
	if changed == 0 {
		return 0, nil
	}
	return changed, cs.Save()
}

// ReadExport parses a collection file written by this app (for importing a backup or
// another device's collection). It returns the file's game ("" for files from before
// multi-game support) and its entries.
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
		if c.ID != "" && c.Quantity > 0 {
			cards = append(cards, c)
		}
	}
	return f.Game, cards, nil
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
