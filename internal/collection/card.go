package collection

import (
	"encoding/json"
	"maps"
	"strings"
)

// Condition is the physical state of a card. The grades are shared by most TCGs.
type Condition string

const (
	ConditionNearMint  Condition = "NM"
	ConditionLightPlay Condition = "LP"
	ConditionModerate  Condition = "MP"
	ConditionHeavyPlay Condition = "HP"
	ConditionDamaged   Condition = "DMG"
)

// Card is one collection entry: a printing of a card in one finish, condition and
// language, with a quantity. The fields here are common to every game; anything a
// particular game adds (Magic's mana cost, colors, Oracle ID, ...) lives in Attrs and
// is stored at the top level of the card's JSON object alongside these fields.
type Card struct {
	ID         string `json:"id"` // the game catalog's printing ID (a Scryfall ID for Magic)
	Name       string `json:"name"`
	Set        string `json:"set"` // set code, uppercase
	SetName    string `json:"set_name"`
	Number     string `json:"collector_number"`
	Rarity     string `json:"rarity"`
	ReleasedAt string `json:"released_at"`
	ImageURI   string `json:"image_uri"`
	TypeLine   string `json:"type_line,omitempty"`
	Text       string `json:"oracle_text,omitempty"` // rules text
	Language   string `json:"lang"`
	// Finishes are the variants the printing exists in (e.g. nonfoil, foil); Finish is
	// the one owned.
	Finishes      []string           `json:"finishes"`
	Finish        string             `json:"finish"`
	Condition     Condition          `json:"condition"`
	Quantity      int                `json:"quantity"`
	PurchasePrice float64            `json:"purchase_price,omitempty"` // per copy
	AcquiredDate  string             `json:"acquired_date,omitempty"`  // YYYY-MM-DD
	Market        map[string]float64 `json:"market,omitempty"`         // USD market price per finish at PricesUpdated
	PricesUpdated string             `json:"prices_updated,omitempty"` // YYYY-MM-DD

	// Attrs holds game-specific fields as raw JSON, keyed by their JSON name. Use the
	// Attr helpers; maps are copied on write so entries never share them.
	Attrs map[string]json.RawMessage `json:"-"`
}

type cardFields Card // same fields, without the custom JSON methods

var knownKeys = func() map[string]bool {
	m := map[string]bool{}
	b, _ := json.Marshal(cardFields{})
	var fields map[string]json.RawMessage
	_ = json.Unmarshal(b, &fields)
	for k := range fields {
		m[k] = true
	}
	// omitempty fields don't appear in a zero marshal
	for _, k := range []string{"type_line", "oracle_text", "purchase_price", "acquired_date", "market", "prices_updated"} {
		m[k] = true
	}
	return m
}()

// UnmarshalJSON reads the common fields and keeps every other key in Attrs.
func (c *Card) UnmarshalJSON(b []byte) error {
	var f cardFields
	if err := json.Unmarshal(b, &f); err != nil {
		return err
	}
	var all map[string]json.RawMessage
	if err := json.Unmarshal(b, &all); err != nil {
		return err
	}
	*c = Card(f)
	c.Attrs = nil
	for k, v := range all {
		if !knownKeys[k] {
			if c.Attrs == nil {
				c.Attrs = map[string]json.RawMessage{}
			}
			c.Attrs[k] = v
		}
	}
	return nil
}

// MarshalJSON writes the common fields and the game-specific Attrs as one object.
func (c Card) MarshalJSON() ([]byte, error) {
	b, err := json.Marshal(cardFields(c))
	if err != nil || len(c.Attrs) == 0 {
		return b, err
	}
	var all map[string]json.RawMessage
	if err := json.Unmarshal(b, &all); err != nil {
		return nil, err
	}
	for k, v := range c.Attrs {
		if !knownKeys[k] { // common fields always win
			all[k] = v
		}
	}
	return json.Marshal(all)
}

// Attr returns a game-specific field's raw JSON, or nil.
func (c Card) Attr(key string) json.RawMessage { return c.Attrs[key] }

// AttrString decodes a string attribute ("" if missing or not a string).
func (c Card) AttrString(key string) string {
	var s string
	_ = json.Unmarshal(c.Attrs[key], &s)
	return s
}

// AttrStrings decodes a string-list attribute.
func (c Card) AttrStrings(key string) []string {
	var s []string
	_ = json.Unmarshal(c.Attrs[key], &s)
	return s
}

// SetAttr stores a game-specific field. A nil value removes it.
func (c *Card) SetAttr(key string, value any) {
	attrs := maps.Clone(c.Attrs) // copy on write: struct copies must not share the map
	if attrs == nil {
		attrs = map[string]json.RawMessage{}
	}
	if value == nil {
		delete(attrs, key)
	} else if b, err := json.Marshal(value); err == nil {
		attrs[key] = b
	}
	if len(attrs) == 0 {
		attrs = nil
	}
	c.Attrs = attrs
}

// ParseCondition normalizes condition labels (TCGplayer, Cardmarket and abbreviations).
func ParseCondition(raw string) Condition {
	c := strings.ToUpper(strings.Join(strings.Fields(strings.NewReplacer("_", " ", "-", " ").Replace(raw)), " "))
	switch {
	case c == "":
		return ConditionNearMint
	case c == "DMG" || c == "D" || c == "PO" || strings.Contains(c, "DAMAGED") || strings.Contains(c, "POOR"):
		return ConditionDamaged
	case c == "HP" || strings.Contains(c, "HEAVY"):
		return ConditionHeavyPlay
	case c == "MP" || strings.Contains(c, "MODERATE") || c == "GD" || c == "GOOD" || c == "PL" || c == "PLAYED":
		return ConditionModerate
	case c == "LP" || c == "SP" || c == "EX" || strings.Contains(c, "LIGHT") || strings.Contains(c, "SLIGHT") || strings.Contains(c, "EXCELLENT"):
		return ConditionLightPlay
	case c == "NM" || c == "M" || c == "MT" || strings.Contains(c, "MINT"):
		return ConditionNearMint
	}
	return ConditionNearMint
}

// CatalogUpdate is fresh catalog data for one printing, from the game's price refresh.
type CatalogUpdate struct {
	Market   map[string]float64 // USD per finish
	TypeLine string
	Text     string
}

// SetInfo describes a set for completion stats.
type SetInfo struct {
	Code       string `json:"code"`
	Name       string `json:"name"`
	CardCount  int    `json:"card_count"`
	ReleasedAt string `json:"released_at"`
}
