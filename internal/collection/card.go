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

// Card is a card in the collection: one printing, with the copies owned grouped by
// finish, then condition (Copies; the card editor's "Your Copies"). The fields here are common to every game;
// anything a particular game adds (Magic's mana cost, colors, Oracle ID, ...) lives in
// Attrs, stored as the card's "attrs" object.
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
	Market    map[string]float64 // USD per finish
	TypeLine  string
	RulesText string
}

// SetInfo describes a set for completion stats.
type SetInfo struct {
	Code       string `json:"code"`
	Name       string `json:"name"`
	CardCount  int    `json:"card_count"`
	ReleasedAt string `json:"released_at"`
}
