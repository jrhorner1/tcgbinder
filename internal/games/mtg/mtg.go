// Package mtg is Magic: The Gathering for the collection app: Scryfall for the card
// catalog and prices, CSV import from other collection apps, and MTGJSON for daily
// price history.
package mtg

import (
	"encoding/json"
	"fmt"
	"log/slog"
	"maps"
	"net/url"
	"path/filepath"
	"slices"
	"strconv"
	"strings"
	"time"

	"github.com/jrhorner1/mtgmanager/internal/collection"
	"github.com/jrhorner1/mtgmanager/internal/game"
	"github.com/jrhorner1/mtgmanager/internal/games/mtg/importer"
	"github.com/jrhorner1/mtgmanager/internal/games/mtg/mtgjson"
	"github.com/jrhorner1/mtgmanager/internal/games/mtg/scryfall"
)

// ID is the game's identifier and data folder name.
const ID = "mtg"

// Game implements game.Game and game.PriceHistorySource for Magic.
type Game struct {
	Scryfall *scryfall.Client
	MTGJSON  *mtgjson.Client
	dir      string
}

var (
	_ game.Game               = (*Game)(nil)
	_ game.PriceHistorySource = (*Game)(nil)
)

// New returns Magic using the live Scryfall and MTGJSON services.
func New() *Game {
	return &Game{Scryfall: scryfall.NewClient(), MTGJSON: mtgjson.NewClient()}
}

func (g *Game) ID() string   { return ID }
func (g *Game) Name() string { return "Magic: The Gathering" }

func (g *Game) Open(dir string) error {
	g.dir = dir
	return nil
}

// --- Catalog -----------------------------------------------------------------

// FromScryfall copies a printing's catalog data into a collection entry. Magic-only
// fields go into Attrs under the same JSON names earlier versions used.
func FromScryfall(sc *scryfall.Card) collection.Card {
	manaCost, colors := sc.ManaCost, sc.Colors
	if manaCost == "" && len(sc.CardFaces) > 0 {
		manaCost = sc.CardFaces[0].ManaCost
	}
	if len(colors) == 0 && len(sc.CardFaces) > 0 {
		seen := map[string]bool{} // double-faced cards only carry colors per face
		for _, f := range sc.CardFaces {
			for _, c := range f.Colors {
				if !seen[c] {
					seen[c] = true
					colors = append(colors, c)
				}
			}
		}
	}
	c := collection.Card{
		ID:            sc.ID,
		Name:          sc.Name,
		Set:           strings.ToUpper(sc.Set),
		SetName:       sc.SetName,
		Number:        sc.Number,
		Rarity:        sc.Rarity,
		ReleasedAt:    sc.ReleasedAt,
		ImageURI:      sc.ImageURL(),
		TypeLine:      sc.TypeLine,
		Text:          sc.Text(),
		Language:      sc.Language,
		Finishes:      sc.Finishes,
		Market:        marketFromPrices(sc.Prices, sc.Finishes),
		PricesUpdated: time.Now().Format("2006-01-02"),
	}
	c.SetAttr("oracle_id", sc.OracleID)
	c.SetAttr("multiverse_ids", sc.MultiverseIDs)
	c.SetAttr("mtgo_id", sc.MTGOID)
	c.SetAttr("arena_id", sc.ArenaID)
	c.SetAttr("tcgplayer_id", sc.TCGPlayerID)
	c.SetAttr("cardmarket_id", sc.CardMarketID)
	c.SetAttr("mana_cost", manaCost)
	c.SetAttr("cmc", sc.CMC)
	c.SetAttr("colors", colors)
	c.SetAttr("color_identity", sc.ColorID)
	c.SetAttr("set_id", sc.SetID)
	return c
}

// marketFromPrices converts Scryfall's USD prices to a price per finish. Printings
// that come in etched but have no etched price use the foil price.
func marketFromPrices(p scryfall.Prices, finishes []string) map[string]float64 {
	m := map[string]float64{}
	put := func(finish, raw string) {
		if v, err := strconv.ParseFloat(raw, 64); err == nil && raw != "" {
			m[finish] = v
		}
	}
	put("nonfoil", p.USD)
	put("foil", p.USDFoil)
	put("etched", p.USDEtched)
	if _, ok := m["etched"]; !ok && slices.Contains(finishes, "etched") {
		put("etched", p.USDFoil)
	}
	if len(m) == 0 {
		return nil
	}
	return m
}

// Upgrade converts entries saved before multi-game support: Scryfall's "prices"
// object becomes the per-finish market price, and old finish spellings are fixed.
func (g *Game) Upgrade(c *collection.Card) bool {
	changed := false
	if raw := c.Attr("prices"); raw != nil {
		var p scryfall.Prices
		if json.Unmarshal(raw, &p) == nil && len(c.Market) == 0 {
			c.Market = marketFromPrices(p, c.Finishes)
		}
		c.SetAttr("prices", nil)
		changed = true
	}
	if _, ok := c.Market["etched"]; ok && !slices.Contains(c.Finishes, "etched") && len(c.Finishes) > 0 {
		// An earlier build gave every card an etched price copied from foil.
		c.Market = maps.Clone(c.Market)
		delete(c.Market, "etched")
		changed = true
	}
	if f := importer.ParseFinish(c.Finish); f != c.Finish {
		c.Finish = f // e.g. "NonFoil" from early versions, or "" -> "nonfoil"
		changed = true
	}
	return changed
}

func (g *Game) Search(text, set, nextPage string) (game.SearchPage, error) {
	q, err := scryfall.BuildSearchQuery(text, set)
	if err != nil {
		return game.SearchPage{}, err
	}
	page, err := g.Scryfall.SearchCards(q, nextPage)
	if err != nil {
		return game.SearchPage{}, err
	}
	out := game.SearchPage{Total: page.Total, NextPage: page.NextPage}
	for i := range page.Cards {
		out.Cards = append(out.Cards, FromScryfall(&page.Cards[i]))
	}
	return out, nil
}

func (g *Game) SearchHints() (string, string) { return "Card name, e.g. Scapeshift", "Set, e.g. M19" }

func (g *Game) Refresh(ids []string, progress func(done, total int)) (map[string]collection.CatalogUpdate, error) {
	var list []string
	for _, id := range ids {
		if scryfall.IsUUID(id) {
			list = append(list, id)
		}
	}
	out := map[string]collection.CatalogUpdate{}
	for start := 0; start < len(list); start += scryfall.CollectionBatchSize {
		end := min(start+scryfall.CollectionBatchSize, len(list))
		bodies := make([]map[string]any, 0, end-start)
		for _, id := range list[start:end] {
			bodies = append(bodies, map[string]any{"id": id})
		}
		resp, err := g.Scryfall.FetchCollection(bodies)
		if err != nil {
			return nil, fmt.Errorf("price refresh failed: %w", err)
		}
		for i := range resp.Data {
			sc := &resp.Data[i]
			out[sc.ID] = collection.CatalogUpdate{Market: marketFromPrices(sc.Prices, sc.Finishes), TypeLine: sc.TypeLine, Text: sc.Text()}
		}
		if progress != nil {
			progress(end, len(list))
		}
	}
	return out, nil
}

func (g *Game) CardText(id string) (string, string, error) {
	sc, err := g.Scryfall.GetCard(id)
	if err != nil {
		return "", "", err
	}
	return sc.TypeLine, sc.Text(), nil
}

// Sets loads Scryfall's set list, cached for a week in the game's data folder.
func (g *Game) Sets() (map[string]collection.SetInfo, error) {
	sets, err := scryfall.LoadSets(g.Scryfall, filepath.Join(g.dir, "scryfall-sets.json"))
	if err != nil {
		return nil, err
	}
	out := make(map[string]collection.SetInfo, len(sets))
	for code, s := range sets {
		out[code] = collection.SetInfo{Code: s.Code, Name: s.Name, CardCount: s.CardCount, ReleasedAt: s.ReleasedAt}
	}
	return out, nil
}

// --- Import ------------------------------------------------------------------

// Import reads a CSV export, matches every row to its exact Scryfall printing and
// returns the entries to add.
func (g *Game) Import(data []byte, progress func(done, total int)) ([]collection.Card, game.ImportReport, error) {
	rows, format, err := importer.ParseCSV(data)
	report := game.ImportReport{Format: format, Rows: len(rows)}
	if err != nil {
		return nil, report, err
	}
	if len(rows) == 0 {
		return nil, report, fmt.Errorf("no card rows found in this %s file", format)
	}
	var approx, missing []string
	var cards []collection.Card
	for _, r := range importer.Resolve(g.Scryfall, rows, progress) {
		if r.Card == nil {
			missing = append(missing, "  "+r.Row.Describe())
			slog.Info("import row not matched", "row", r.Row.Describe(), "reason", r.Err)
			continue
		}
		if r.Approximate {
			approx = append(approx, fmt.Sprintf("  %s → %s · %s", r.Row.Describe(), strings.ToUpper(r.Card.Set), r.Card.Number))
		}
		entry := FromScryfall(r.Card)
		entry.Finish = importer.ChooseFinish(r.Row.Finish, r.Card.Finishes)
		entry.Condition = r.Row.Condition
		entry.Quantity = r.Row.Quantity
		entry.PurchasePrice = r.Row.PurchasePrice
		entry.AcquiredDate = r.Row.AcquiredDate
		cards = append(cards, entry)
	}
	if len(approx) > 0 {
		report.Notes = append(report.Notes, fmt.Sprintf("Matched by name only, so the printing may differ (%d) — please check:", len(approx)))
		report.Notes = append(report.Notes, approx...)
	}
	if len(missing) > 0 {
		report.Notes = append(report.Notes, fmt.Sprintf("Not imported — no match on Scryfall (%d):", len(missing)))
		report.Notes = append(report.Notes, missing...)
	}
	return cards, report, nil
}

// --- Presentation ------------------------------------------------------------

func (g *Game) Finishes() []string { return []string{"nonfoil", "foil", "etched"} }

func (g *Game) Rarities() []string {
	return []string{"common", "uncommon", "rare", "mythic", "special", "bonus"}
}

const (
	colorMulticolor = "Multicolor"
	colorColorless  = "Colorless"
)

func (g *Game) Filters() []game.Filter {
	return []game.Filter{{
		Name:    "Color",
		Options: []string{"W", "U", "B", "R", "G", colorMulticolor, colorColorless},
		Match:   matchesColor,
	}}
}

// matchesColor: a single color also matches multicolor cards that include it.
func matchesColor(c collection.Card, want string) bool {
	colors := c.AttrStrings("colors")
	switch want {
	case colorColorless:
		return len(colors) == 0
	case colorMulticolor:
		return len(colors) > 1
	}
	for _, col := range colors {
		if strings.EqualFold(col, want) {
			return true
		}
	}
	return false
}

func (g *Game) Details(c collection.Card) []game.Row {
	var rows []game.Row
	if mc := c.AttrString("mana_cost"); mc != "" {
		rows = append(rows, game.Row{Label: "Mana cost", Value: mc})
	}
	return rows
}

func (g *Game) CardLink(c collection.Card) (string, *url.URL) {
	u, _ := url.Parse(fmt.Sprintf("https://scryfall.com/card/%s/%s",
		url.PathEscape(strings.ToLower(c.Set)), url.PathEscape(c.Number)))
	return "View on Scryfall", u
}
