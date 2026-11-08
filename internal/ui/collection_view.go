package ui

import (
	"fmt"
	"strings"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"github.com/jrhorner1/tcgbinder/internal/collection"
	"github.com/jrhorner1/tcgbinder/internal/game"
	"github.com/jrhorner1/tcgbinder/internal/service"
)

// Tree node IDs carry the game ID so games never collide:
//
//	home
//	game:<game>                the game's cards, in the Card order setting's order
//	card:<game>:<card key>
//	tools
//	settings
const (
	nodeHome     = "home"
	nodeTools    = "tools"
	nodeSettings = "settings"
	prefixGame   = "game:"
	prefixCard   = "card:"
)

func gameNode(gameID string) string { return prefixGame + gameID }
func cardNode(gameID string, c collection.Card) string {
	return prefixCard + gameID + ":" + collection.CardKey(c)
}

// parseNode splits a node ID into its kind prefix, game ID and remainder.
func parseNode(id string) (kind, gameID, rest string) {
	for _, p := range []string{prefixGame, prefixCard} {
		if strings.HasPrefix(id, p) {
			gameID, rest, _ = strings.Cut(strings.TrimPrefix(id, p), ":")
			return p, gameID, rest
		}
	}
	return id, "", ""
}

// gameState is one game's collection, price history and the tree data built from it.
type gameState struct {
	svc    *service.Game
	game   game.Game
	store  *collection.Store
	prices *collection.PriceHistory
	sets   map[string]collection.SetInfo // nil until loaded
	cards  []collection.Card             // every card

	refreshing, loadingHistory bool

	// Tree data, rebuilt by applyFilter.
	filtered    []collection.Card          // matching cards, in the Card order setting's order
	cardIDs     []string                   // their node IDs, in the same order
	cardNodes   map[string]collection.Card // card node ID -> card
	cardCount   int                        // copies in the filtered view
	uniqueCount int                        // distinct printings in the filtered view
	setCount    int                        // sets in the filtered view
}

// searchDelay is how long typing must pause before the collection is filtered.
const searchDelay = 150 * time.Millisecond

// collectionView holds the search and filter controls. Filters other than the
// search text apply to the active game, whose menus they come from.
type collectionView struct {
	query          collection.Query
	setCodeByLabel map[string]string
	extra          map[string]string // game filter name -> chosen option

	searchEntry  *widget.Entry
	filterRow    *fyne.Container
	setSelect    *widget.Select
	selects      []*widget.Select // every filter menu, for Clear
	summaryLabel *widget.Label
}

// newFilterSelect builds a select whose placeholder names the filter; choosing "Any"
// clears it back to the placeholder so the user can always see what each control is.
func newFilterSelect(placeholder string, options []string, apply func(string)) *widget.Select {
	var sel *widget.Select
	sel = widget.NewSelect(append([]string{collection.FilterAny}, options...), func(s string) {
		if s == collection.FilterAny {
			sel.ClearSelected() // re-enters with "" below
			return
		}
		apply(s)
	})
	sel.PlaceHolder = placeholder
	return sel
}

func titleCaseAll(opts []string) []string {
	out := make([]string, len(opts))
	for i, o := range opts {
		out[i] = titleCase(o)
	}
	return out
}

// buildFilterBar creates the search / filter controls shown above the tree.
func (ui *App) buildFilterBar() fyne.CanvasObject {
	v := &ui.view
	v.query.SortBy = collection.SortNumber // within each set, cards are listed by collector number

	v.searchEntry = widget.NewEntry()
	v.searchEntry.SetPlaceHolder("Search name, set, or collector number")
	v.searchEntry.ActionItem = widget.NewIcon(theme.SearchIcon())
	// Filter once typing pauses, not on every keystroke: regrouping a large collection
	// takes long enough on a phone to make typing lag.
	var pending *time.Timer
	v.searchEntry.OnChanged = func(s string) {
		if pending != nil {
			pending.Stop()
		}
		pending = time.AfterFunc(searchDelay, func() {
			runOnUI(func() {
				if v.searchEntry.Text == s { // the latest text: apply it
					v.query.Text = s
					ui.applyFilter()
				}
			})
		})
	}

	clearButton := widget.NewButtonWithIcon("", theme.ContentClearIcon(), func() {
		v.searchEntry.SetText("")
		for _, s := range v.selects {
			s.ClearSelected()
		}
	})

	v.summaryLabel = widget.NewLabel("")
	v.summaryLabel.TextStyle = fyne.TextStyle{Italic: true}
	v.filterRow = container.NewGridWithColumns(1)
	ui.rebuildFilterMenus()

	searchRow := container.NewBorder(nil, nil, nil, clearButton, v.searchEntry)
	return container.NewVBox(searchRow, v.filterRow, v.summaryLabel, widget.NewSeparator())
}

// rebuildFilterMenus makes the filter menus for the active game: set, the game's own
// filters (e.g. Magic's colors), rarity and finish.
func (ui *App) rebuildFilterMenus() {
	v := &ui.view
	g := ui.g.game
	v.query.Set, v.query.Rarity, v.query.Finish = "", "", ""
	v.query.RarityOrder = g.Rarities()
	v.extra = map[string]string{}
	filters := g.Filters()
	v.query.Match = func(c collection.Card) bool {
		for _, f := range filters {
			if opt := v.extra[f.Name]; !collection.IsAny(opt) && !f.Match(c, opt) {
				return false
			}
		}
		return true
	}

	v.setSelect = newFilterSelect("Set", nil, func(label string) {
		v.query.Set = v.setCodeByLabel[label]
		ui.applyFilter()
		if v.query.Set != "" && !isMobile() { // beside the set's cards, its page
			ui.openSet(v.query.Set)
		}
	})
	menus := []fyne.CanvasObject{v.setSelect}
	v.selects = []*widget.Select{v.setSelect}
	for _, f := range filters {
		f := f
		sel := newFilterSelect(f.Name, f.Options, func(s string) {
			v.extra[f.Name] = s
			ui.applyFilter()
		})
		menus = append(menus, sel)
		v.selects = append(v.selects, sel)
	}
	rarity := newFilterSelect("Rarity", titleCaseAll(g.Rarities()), func(s string) {
		v.query.Rarity = s
		ui.applyFilter()
	})
	finishByName := map[string]string{}
	var finishNames []string
	for _, f := range g.Finishes() {
		finishByName[g.FinishName(f)] = f
		finishNames = append(finishNames, g.FinishName(f))
	}
	finish := newFilterSelect("Finish", finishNames, func(s string) {
		v.query.Finish = finishByName[s]
		ui.applyFilter()
	})
	menus = append(menus, rarity, finish)
	v.selects = append(v.selects, rarity, finish)

	v.filterRow.Layout = container.NewGridWithColumns(cols(len(menus))).Layout
	v.filterRow.Objects = menus
	v.filterRow.Refresh()
	ui.refreshSetOptions()
}

func (v *collectionView) filtering() bool {
	q := v.query
	if strings.TrimSpace(q.Text) != "" || !collection.IsAny(q.Set) || !collection.IsAny(q.Rarity) || !collection.IsAny(q.Finish) {
		return true
	}
	for _, opt := range v.extra {
		if !collection.IsAny(opt) {
			return true
		}
	}
	return false
}

// applyFilter regroups each game's filtered cards into the tree and refreshes it.
func (ui *App) applyFilter() {
	v := &ui.view
	for _, gs := range ui.games {
		q := collection.Query{Text: v.query.Text, RarityOrder: gs.game.Rarities()}
		if gs == ui.g {
			q = v.query // menus come from the active game
		}
		gs.filtered = collection.SortCards(collection.ApplyQuery(gs.cards, q), ui.settings.Get().CardOrder)
		gs.cardIDs = make([]string, len(gs.filtered))
		gs.cardNodes = make(map[string]collection.Card, len(gs.filtered))
		sets := map[string]bool{}
		gs.cardCount = 0
		for i, c := range gs.filtered {
			id := cardNode(gs.game.ID(), c)
			gs.cardIDs[i], gs.cardNodes[id] = id, c
			gs.cardCount += c.CopyCount()
			sets[strings.ToUpper(c.Set)] = true
		}
		gs.uniqueCount, gs.setCount = collection.UniqueCards(gs.filtered), len(sets)
	}

	if ui.tree != nil {
		ui.tree.Refresh()
		for _, gs := range ui.games { // the cards are always in view under their game
			ui.tree.OpenBranch(gameNode(gs.game.ID()))
		}
	}
	if v.summaryLabel != nil {
		gs := ui.g
		counts := fmt.Sprintf("%s · %s · %s", plural(gs.uniqueCount, "unique card", "unique cards"),
			plural(gs.cardCount, "total card", "total cards"), plural(gs.setCount, "set", "sets"))
		if v.filtering() {
			counts = "Matching: " + counts
		}
		v.summaryLabel.SetText(counts)
	}
}

func (ui *App) gameByID(id string) *gameState {
	for _, gs := range ui.games {
		if gs.game.ID() == id {
			return gs
		}
	}
	return nil
}

// activate makes gs the game the filters, Add and Import apply to.
func (ui *App) activate(gs *gameState) {
	if gs == nil || gs == ui.g {
		return
	}
	ui.g = gs
	if ui.view.filterRow != nil {
		ui.rebuildFilterMenus()
	}
	ui.applyFilter()
}

// buildTree creates the navigation tree: Home, each game with its cards, Tools and
// Settings.
func (ui *App) buildTree() *widget.Tree {
	tree := widget.NewTree(
		func(id widget.TreeNodeID) []widget.TreeNodeID {
			if id == "" {
				out := []string{nodeHome}
				for _, gs := range ui.games {
					out = append(out, gameNode(gs.game.ID()))
				}
				return append(out, nodeTools, nodeSettings)
			}
			if kind, gameID, _ := parseNode(id); kind == prefixGame {
				if gs := ui.gameByID(gameID); gs != nil {
					return gs.cardIDs
				}
			}
			return nil
		},
		func(id widget.TreeNodeID) bool {
			kind, _, _ := parseNode(id)
			return id == "" || kind == prefixGame
		},
		func(branch bool) fyne.CanvasObject {
			name := widget.NewLabel("")
			name.Truncation = fyne.TextTruncateEllipsis
			detail := widget.NewLabel("")
			return container.NewBorder(nil, nil, widget.NewIcon(nil), detail, name)
		},
		func(id widget.TreeNodeID, branch bool, obj fyne.CanvasObject) {
			row := obj.(*fyne.Container)
			// Border orders its objects: center, then leading, then trailing.
			name := row.Objects[0].(*widget.Label)
			icon := row.Objects[1].(*widget.Icon)
			detail := row.Objects[2].(*widget.Label)

			kind, gameID, _ := parseNode(id)
			gs := ui.gameByID(gameID)
			switch {
			case id == nodeHome:
				icon.SetResource(theme.HomeIcon())
				name.SetText("Home")
				detail.SetText("")
			case id == nodeTools:
				icon.SetResource(theme.MenuIcon())
				name.SetText("Tools")
				detail.SetText("")
			case id == nodeSettings:
				icon.SetResource(theme.SettingsIcon())
				name.SetText("Settings")
				detail.SetText("")
			case gs == nil:
				name.SetText("")
				detail.SetText("")
			case kind == prefixGame:
				icon.SetResource(theme.StorageIcon())
				name.SetText(gs.game.Name())
				detail.SetText(plural(gs.cardCount, "card", "cards")) // as Home's Cards tile
			default:
				c := gs.cardNodes[id]
				icon.SetResource(nil)
				name.SetText(c.Name)
				detail.SetText(setNumber(c.Set, c.CollectorNumber) + " · " + copiesSummary(c, gs.game))
			}
			row.Refresh() // re-run the Border layout now that the labels have real widths
		},
	)

	tree.OnSelected = func(id widget.TreeNodeID) {
		ui.selected = id
		kind, gameID, _ := parseNode(id)
		if gs := ui.gameByID(gameID); gs != nil {
			ui.activate(gs)
		}
		switch {
		case id == nodeHome || kind == prefixGame:
			ui.showHome()
			ui.showDetail("Home")
		case id == nodeTools:
			ui.showTools()
			ui.showDetail("Tools")
		case id == nodeSettings:
			ui.showSettings()
			ui.showDetail("Settings")
		case kind == prefixCard:
			if c, ok := ui.g.cardNodes[id]; ok {
				ui.renderEditor(c)
				ui.showDetail(c.Name)
			}
		}
	}
	return tree
}

// openSet shows a set's page (completion, value, missing cards).
func (ui *App) openSet(code string) {
	code = strings.ToUpper(code)
	title := code
	if info, ok := ui.g.sets[code]; ok && info.Name != "" {
		title = info.Name
	} else {
		for _, c := range ui.g.cards {
			if strings.EqualFold(c.Set, code) && c.SetName != "" {
				title = c.SetName
				break
			}
		}
	}
	if !isMobile() {
		ui.tree.UnselectAll() // the page isn't in the tree
	}
	ui.showSet(code)
	ui.showDetail(title)
}

// refreshSetOptions rebuilds the set filter choices from the active game's sets,
// preserving the current selection when that set still exists.
func (ui *App) refreshSetOptions() {
	v := &ui.view
	if v.setSelect == nil {
		return
	}
	labels, byLabel := collection.SetOptions(ui.g.cards)
	v.setCodeByLabel = byLabel
	current := v.setSelect.Selected
	v.setSelect.SetOptions(labels)
	if _, ok := byLabel[current]; current != "" && !ok {
		v.setSelect.ClearSelected()
	}
}

// plural formats a count with the right noun, e.g. "1 card", "3 cards".
func plural(n int, one, many string) string {
	if n == 1 {
		return fmt.Sprintf("%d %s", n, one)
	}
	return fmt.Sprintf("%d %s", n, many)
}
