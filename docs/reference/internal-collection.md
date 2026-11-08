# `internal/collection`

```
package collection // import "github.com/jrhorner1/tcgbinder/internal/collection"

Package collection holds the game-neutral collection model: cards, the JSON file
store (one per game), price history, filtering, and value/completion statistics.

CONSTANTS

const (
	OrderName   = "name"   // by name (DefaultOrder)
	OrderNewest = "newest" // by set, newest release first, then collector number
	OrderOldest = "oldest" // by set, oldest release first, then collector number
	OrderValue  = "value"  // most valuable first (every copy), then by name
)
    Card orders for the collection list (the Card order setting).

const FilterAny = "Any"
    FilterAny is the "no filter" choice in filter menus.


VARIABLES

var Conditions = []Condition{ConditionNearMint, ConditionLightPlay, ConditionModerate, ConditionHeavyPlay, ConditionDamaged}
    Conditions in order from best to worst.

var SortFields = []SortField{SortName, SortSet, SortNumber, SortRarity, SortQuantity, SortPrice, SortAcquired, SortReleased}
    SortFields lists the sort options in the order they appear in the UI.


FUNCTIONS

func CardKey(c Card) string
    CardKey identifies a collection card: one per printing, with its copies
    grouped by finish and condition in Copies.

func DefaultOrder(a, b Card) int
    defaultOrder is the tiebreaker: name, then set, then collector number,
    then ID for determinism.

func IsAny(s string) bool
func MarketPrice(c Card, finish string) (price float64, ok bool)
    MarketPrice is the current per-copy USD market price of a card in a finish,
    from the prices stored at the last refresh. ok is false when the catalog had
    no price.

func SetOptions(cards []Card) (labels []string, codeByLabel map[string]string)
    SetOptions returns FilterAny followed by the distinct set codes present in
    cards, labelled "CODE — Set Name" and sorted by set name.

func UniqueCards(cards []Card) int
    UniqueCards counts distinct printings (catalog IDs): several copies,
    finishes or conditions of the same printing count once.


TYPES

type BulkChange struct {
	Condition    *Condition // move every copy to this condition
	Finish       *string    // move every copy to this finish, if the printing comes in it
	PricePaid    *float64
	AcquiredDate *string
	Delete       bool
}
    BulkChange describes an edit applied to many cards at once. Nil fields are
    left unchanged; each applies to every copy of the card.

func (change BulkChange) Apply(c Card) (Card, string)
    Apply returns c with the change made (except Delete). ok is false, with a
    reason, when the finish change can't apply because the printing doesn't come
    in it.

type BulkResult struct {
	Updated, Deleted int
	Skipped          []string // cards the change didn't apply to, with why
}
    BulkResult reports what a bulk edit did.

type Card struct {
	ID              string `json:"id"` // the game catalog's printing ID (a Scryfall ID for Magic)
	Name            string `json:"name"`
	Set             string `json:"set"` // set code, uppercase
	SetName         string `json:"set_name"`
	CollectorNumber string `json:"collector_number"`
	Rarity          string `json:"rarity"`
	ReleasedAt      string `json:"released_at"`
	ImageURI        string `json:"image_uri"`
	TypeLine        string `json:"type_line,omitempty"`
	RulesText       string `json:"oracle_text,omitempty"` // rules text
	Language        string `json:"lang"`
	// Finishes are the variants the printing exists in (e.g. nonfoil, foil).
	Finishes []string `json:"finishes"`
	// Copies are the copies owned: finish -> condition -> purchases.
	Copies        Copies             `json:"copies,omitempty"`
	Market        map[string]float64 `json:"market,omitempty"`         // USD market price per finish at PricesUpdated
	PricesUpdated string             `json:"prices_updated,omitempty"` // YYYY-MM-DD

	// Attrs holds game-specific fields as raw JSON, keyed by their JSON name. Use the
	// Attr helpers; maps are copied on write so cards never share them.
	Attrs map[string]json.RawMessage `json:"attrs,omitempty"`
}
    Card is a card in the collection: one printing, with the copies owned
    grouped by finish, then condition (Copies; the card editor's "Your Copies").
    The fields here are common to every game; anything a particular game adds
    (Magic's mana cost, colors, Oracle ID, ...) lives in Attrs, stored as the
    card's "attrs" object.

func ApplyQuery(cards []Card, q Query) []Card
    ApplyQuery returns a new slice containing the cards that match q, ordered
    per q. The input slice is not modified.

func Missing(setCards, owned []Card) []Card
    Missing returns the cards of a set that aren't owned (by collector number).

func ReadExport(data []byte) (game string, cards []Card, err error)
    ReadExport parses a collection file written by this app (for importing a
    backup or another device's collection). It returns the file's game and its
    cards.

func SortCards(cards []Card, order string) []Card
    SortCards returns the cards in one of the Card orders, leaving the input as
    it is.

func WithPrinting(owned, printing Card) Card
    WithPrinting moves an owned card to another printing of the same card:
    catalog data comes from printing and every purchase is kept. Copies in a
    finish the new printing doesn't come in move to its first finish.

func (c *Card) AddPurchase(finish string, cond Condition, l Purchase)
    AddPurchase adds a purchase to the card. It joins an existing purchase in
    the same finish and condition when the price paid and acquired date match.

func (c Card) Attr(key string) json.RawMessage
    Attr returns a game-specific field's raw JSON, or nil.

func (c Card) AttrString(key string) string
    AttrString decodes a string attribute ("" if missing or not a string).

func (c Card) AttrStrings(key string) []string
    AttrStrings decodes a string-list attribute.

func (c Card) BestPrice() (float64, bool)
    BestPrice is the highest market price among the finishes owned: one copy's
    value.

func (c Card) ConditionCopyCount(finish string, cond Condition) int
    ConditionCopyCount is the number of copies owned in one finish and
    condition.

func (c Card) CopyCount() int
    CopyCount is the total number of copies owned.

func (c Card) EarliestAcquired() string
    EarliestAcquired is the earliest acquired date among the copies, or "".

func (c Card) FinishCopyCount(finish string) int
    FinishCopyCount is the number of copies owned in one finish.

func (c Card) HasCondition(cond Condition) bool
    HasCondition reports whether any copies are in cond.

func (c Card) HasFinish(finish string) bool
    HasFinish reports whether any copies are owned in finish (case-insensitive).

func (c Card) MarketValue() (total float64, unpriced int)
    MarketValue is the market value of every copy (each at its finish's price);
    unpriced counts copies whose finish has no price.

func (c Card) OwnedFinishes(finishOrder ...string) []string
    OwnedFinishes lists the finishes with copies, in finishOrder.

func (c Card) PricePaid() (total float64, unknown int)
    PricePaid is the total price paid for copies with a known price; unknown
    counts copies without one.

func (c Card) PurchaseRows(finishOrder ...string) []PurchaseRow
    PurchaseRows lists the card's copies, finishes in finishOrder (others after,
    by name) and conditions best first.

func (c Card) Purchases(finish string, cond Condition) []Purchase
    Purchases lists the purchases in one finish and condition, oldest first.

func (c *Card) SetAttr(key string, value any)
    SetAttr stores a game-specific field. A nil value removes it.

func (c *Card) SetPurchases(finish string, cond Condition, purchases []Purchase)
    SetPurchases replaces the purchases in one finish and condition; purchases
    with no copies are dropped, and an empty list removes the condition.

type CardValue struct {
	Card  Card
	Each  float64
	Total float64
}
    CardValue is a card with its total current value.

func TopByEach(cards []Card, n int) []CardValue
    TopByEach ranks priced cards by the value of one copy (their most valuable
    owned finish), most valuable first.

type CatalogUpdate struct {
	Market    map[string]float64 // USD per finish
	TypeLine  string
	RulesText string
}
    CatalogUpdate is fresh catalog data for one printing, from the game's price
    refresh.

type Condition string
    Condition is the physical state of a card. The grades are shared by most
    TCGs.

const (
	ConditionNearMint  Condition = "NM"
	ConditionLightPlay Condition = "LP"
	ConditionModerate  Condition = "MP"
	ConditionHeavyPlay Condition = "HP"
	ConditionDamaged   Condition = "DMG"
)
func ParseCondition(raw string) Condition
    ParseCondition normalizes condition labels (TCGplayer, Cardmarket and
    abbreviations).

type Copies map[string]map[Condition][]Purchase
    Copies are the copies of one printing owned: finish -> condition ->
    purchases. Each purchase keeps its own price paid and acquired date.

type DailyPrices struct {
	From  string  `json:"from"` // YYYY-MM-DD of Cents[0]
	Cents []int32 `json:"cents"`
	// Has unexported fields.
}
    DailyPrices is one printing's prices in one finish, one per day: Cents[i]
    is the USD price in cents on From + i days, or -1 for a day without a price.
    Prices are recorded for nearly every day, so an array is far smaller and
    faster than a map by date, and whole cents (which prices always are) encode
    much faster than decimals.

type PriceHistory struct {
	HistoryLoaded string `json:"history_loaded,omitempty"` // date price history was last loaded from MTGJSON
	// Cards maps Scryfall ID -> finish -> that printing's daily prices.
	Cards map[string]map[string]*DailyPrices `json:"cards"`
	// Has unexported fields.
}
    PriceHistory holds daily market prices per printing and finish, so the
    collection's real value can be charted over time. It is filled by a one-time
    MTGJSON download (Load price history) (up to 90 days back) and by recording
    Scryfall prices each day the app is used.

func OpenPriceHistory(path string) (*PriceHistory, error)
    OpenPriceHistory reads the history file at path; a missing file gives
    an empty history. A file that can't be parsed is never overwritten:
    it is renamed to "<path>.unreadable-<timestamp>" so it can be recovered, and
    a fresh history is returned together with an error describing what happened.

func (h *PriceHistory) Days(id, finish string) int
    Days is the number of days with a price for a printing and finish.

func (h *PriceHistory) HistoryLoadedOn() string
    HistoryLoadedOn returns the date of the last price history load, or "".

func (h *PriceHistory) MarkHistoryLoaded(date string)
    MarkHistoryLoaded records that price history was loaded on date.

func (h *PriceHistory) MarketSeries(cards []Card, from, to time.Time) []TimePoint
    MarketSeries returns the collection's market value for each day from `from`
    through `to` on which any price is recorded, plus the range's end points:
    the copies owned that day (by acquired date; undated copies always count)
    times that day's price for their finish. A card without a price that day
    uses its nearest earlier recorded price, then its nearest later one, then
    its current Scryfall price, so gaps in the data don't read as value drops.

func (h *PriceHistory) Merge(id string, byFinish map[string]map[string]float64)
    Merge adds a historyLoaded series for one printing. Dates already recorded
    keep their value (prices the app recorded itself are not replaced),
    and nothing is removed, so days older than MTGJSON's 90-day window stay in
    the history.

func (h *PriceHistory) Points() int
    Points is the number of recorded daily prices.

func (h *PriceHistory) Price(id, finish, date string) (float64, bool)
    Price returns the price recorded for a printing and finish on date
    (YYYY-MM-DD).

func (h *PriceHistory) RecordCurrent(cards []Card, date string)
    RecordCurrent stores each card's current market prices (every finish) for
    date.

func (h *PriceHistory) Save() error
    Save writes the history atomically: a temp file is written, synced to disk,
    then renamed over the old file. Saves are serialized so a slower save can
    never replace a newer one, and a save that would hold fewer prices than were
    loaded is refused — history is only ever added to.

func (h *PriceHistory) Set(id, finish, date string, price float64)
    Set records one price.

func (h *PriceHistory) Start() time.Time
    Start returns the earliest date with any recorded price, or zero time.

type Purchase struct {
	Quantity     int     `json:"quantity"`
	PricePaid    float64 `json:"price_paid,omitempty"`    // per copy
	AcquiredDate string  `json:"acquired_date,omitempty"` // YYYY-MM-DD
}
    Purchase is one purchase: copies of a printing in one finish and condition,
    bought for the same price on the same date.

type PurchaseRow struct {
	Finish    string
	Condition Condition
	Purchase
}
    PurchaseRow is one purchase of a card, with its finish and condition.

type Query struct {
	Text   string // whitespace-separated terms; every term must match name, set code, set name or collector number
	Set    string // set code (case-insensitive); "" or FilterAny for all
	Rarity string // "" or FilterAny for all
	Finish string // "" or FilterAny for all, otherwise the finish (case-insensitive)
	// Match is an extra, game-specific condition (e.g. Magic's color filter); nil for none.
	Match func(Card) bool
	// RarityOrder ranks rarities for SortRarity, lowest first; others sort after them.
	RarityOrder []string
	SortBy      SortField // defaults to SortName
	Descending  bool
}
    Query describes how to narrow and order the collection view. Zero values
    mean "no filter", sorted by name ascending.

type SetInfo struct {
	Code       string `json:"code"`
	Name       string `json:"name"`
	CardCount  int    `json:"card_count"`
	ReleasedAt string `json:"released_at"`
}
    SetInfo describes a set for completion stats.

type SetStats struct {
	Code        string
	Name        string
	Released    string // YYYY-MM-DD
	UniqueCards int
	Cards       int
	Collected   int // distinct collector numbers owned, any finish/condition
	SetSize     int // cards in the set per Scryfall; 0 if unknown
	PricePaid   float64
	MarketValue float64
}
    SetStats summarizes the owned cards of one set.

func (s SetStats) Completion() float64
    Completion is the owned fraction of the set (0..1), or -1 when the set size
    is unknown.

type SortField string
    SortField identifies a column the collection list can be ordered by.

const (
	SortName     SortField = "Name"
	SortSet      SortField = "Set"
	SortNumber   SortField = "Collector #"
	SortRarity   SortField = "Rarity"
	SortQuantity SortField = "Quantity"
	SortPrice    SortField = "Purchase Price"
	SortAcquired SortField = "Acquired Date"
	SortReleased SortField = "Release Date"
)
type Stats struct {
	Cards, UniqueCards, Sets int
	PricePaid                float64 // total price paid, for copies with a known price
	PricePaidUnknown         int     // copies with no price paid recorded
	MarketValue              float64 // total current market value
	Unpriced                 int     // copies with no market price
	PricesAsOf               string  // oldest price refresh date across cards
	BySet                    []SetStats
	TopCards                 []CardValue
	PricePaidHistory         []TimePoint // cumulative price paid by acquisition date, through today
	// ValueHistory is the value of the cards owned on each date, priced at today's
	// market prices (by acquisition date, through today). Scryfall has no historical
	// prices, so this shows how the collection grew, not how prices moved.
	ValueHistory []TimePoint
}
    CollectionStats is everything the Home dashboard shows.

func ComputeStats(cards []Card, sets map[string]SetInfo) Stats
    ComputeStats aggregates the collection. sets may be nil when set sizes
    aren't loaded.

type Store struct {
	Game    string          `json:"game,omitempty"` // ID of the game this file belongs to
	Cards   map[string]Card `json:"cards"`
	History []ValueSnapshot `json:"history,omitempty"` // daily totals for the value-over-time chart
	// Has unexported fields.
}
    Store is one game's collection, persisted as a JSON file. Each game has its
    own.

func NewStore(filename string) (*Store, error)
    NewCollectionStore initializes a store pointing to a local file path.

func OpenStore(path, game string) (*Store, error)
    OpenStore opens (or creates) the collection file for game at path. A file
    that belongs to a different game is refused; a new file is claimed for this
    one.

func (cs *Store) AddCards(cards []Card) (created, merged int, err error)
    AddCards adds copies to the collection, merging into the card for the same
    printing when there is one, and saves once. It reports how many new cards
    were created and how many cards were added to existing ones.

func (cs *Store) BulkEdit(keys []string, change BulkChange) (BulkResult, error)
    BulkEdit applies change to the cards with the given keys and saves.

func (cs *Store) FilePath() string
    FilePath is the absolute path of the collection file.

func (cs *Store) Get(key string) (Card, bool)
    Get returns the card with the given CardKey.

func (cs *Store) GetCollection() []Card
    GetCollection returns a copied slice of the active collection safely
    isolating raw thread access.

func (cs *Store) IDs() []string
    IDs returns the distinct printing IDs in the collection.

func (cs *Store) Load() error
    Load populates the in-memory map from the local storage file.

func (cs *Store) PricesOlderThan(days int) bool
    PricesOlderThan reports whether any card's market prices are at least days
    old (1: from before today; 7: from a week ago or earlier).

func (cs *Store) PricesStale() bool
    PricesStale reports whether any card's market prices were last refreshed
    before today.

func (cs *Store) RecordSnapshot() error
    RecordSnapshot saves today's totals to the value history (replacing any
    earlier snapshot from today), using the stored market prices.

func (cs *Store) RemoveCard(key string) error
    RemoveCard drops the card with the given CardKey.

func (cs *Store) Save() error
    Save flushes the current state of the collection to disk using an atomic
    write.

func (cs *Store) SaveCard(oldKey string, card Card) (newKey string, merged bool, err error)
    SaveCard saves an edited card that was stored under oldKey (its copies
    replace the old ones). If the card moved to another printing that is already
    in the collection, its copies merge into that card. No copies left deletes
    the card.

func (cs *Store) UpdateCatalog(updates map[string]CatalogUpdate, date string) (int, error)
    UpdateCatalog applies fresh prices (and missing rules text) from the game's
    catalog, records today's value snapshot and saves. It returns the cards
    updated.

func (cs *Store) UpsertCard(card Card) error
    UpsertCard saves card in place of the one for its printing. No copies
    removes it.

func (cs *Store) ValueHistory() []ValueSnapshot
    ValueHistory returns a copy of the recorded daily snapshots, oldest first.

type TimePoint struct {
	Date  time.Time
	Value float64
}
    TimePoint is one value on a time series.

type ValueSnapshot struct {
	Date        string  `json:"date"` // YYYY-MM-DD
	MarketValue float64 `json:"market_value"`
	PricePaid   float64 `json:"price_paid"`
	Cards       int     `json:"cards"`
}
    ValueSnapshot records the collection's totals on one day, for the
    value-over-time chart.

```
