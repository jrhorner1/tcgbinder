package scryfall

// CardPrices mirrors Scryfall's prices object (values are decimal strings; "" when unknown).
type Prices struct {
	USD       string `json:"usd,omitempty"`
	USDFoil   string `json:"usd_foil,omitempty"`
	USDEtched string `json:"usd_etched,omitempty"`
	EUR       string `json:"eur,omitempty"`
	EURFoil   string `json:"eur_foil,omitempty"`
	Tix       string `json:"tix,omitempty"`
}

type imageURIs struct {
	Normal string `json:"normal"`
}

// ScryfallCard is the subset of a Scryfall card object the app uses.
type Card struct {
	ID            string    `json:"id"`
	OracleID      string    `json:"oracle_id"`
	MultiverseIDs []int     `json:"multiverse_ids"`
	MTGOID        int       `json:"mtgo_id"`
	ArenaID       int       `json:"arena_id"`
	TCGPlayerID   int       `json:"tcgplayer_id"`
	CardMarketID  int       `json:"cardmarket_id"`
	Name          string    `json:"name"`
	Language      string    `json:"lang"`
	ReleasedAt    string    `json:"released_at"`
	OracleText    string    `json:"oracle_text"`
	TypeLine      string    `json:"type_line"`
	CMC           float64   `json:"cmc"`
	ImageURIs     imageURIs `json:"image_uris"`
	CardFaces     []struct {
		Name       string    `json:"name"`
		ManaCost   string    `json:"mana_cost"`
		OracleText string    `json:"oracle_text"`
		Colors     []string  `json:"colors"`
		ImageURIs  imageURIs `json:"image_uris"`
	} `json:"card_faces"`
	ManaCost string   `json:"mana_cost"`
	Colors   []string `json:"colors"`
	ColorID  []string `json:"color_identity"`
	Finishes []string `json:"finishes"`
	SetID    string   `json:"set_id"`
	Set      string   `json:"set"`
	SetName  string   `json:"set_name"`
	Number   string   `json:"collector_number"`
	Rarity   string   `json:"rarity"`
	Prices   Prices   `json:"prices"`
}
