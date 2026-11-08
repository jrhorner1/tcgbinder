package ui

import (
	"fmt"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"github.com/jrhorner1/mtgmanager/internal/collection"
	"github.com/jrhorner1/mtgmanager/internal/game"
	"github.com/jrhorner1/mtgmanager/internal/service"
)

// Tree node IDs carry the game ID so games never collide:
//
//	home
//	game:<game>
//	set:<game>:<set code>
//	card:<game>:<entry key>
const (
	nodeHome   = "home"
	prefixGame = "game:"
	prefixSet  = "set:"
	prefixCard = "card:"
)

func gameNode(gameID string) string      { return prefixGame + gameID }
func setNode(gameID, code string) string { return prefixSet + gameID + ":" + code }
func cardNode(gameID string, c collection.Card) string {
	return prefixCard + gameID + ":" + collection.EntryKey(c)
}

// parseNode splits a node ID into its kind prefix, game ID and remainder.
func parseNode(id string) (kind, gameID, rest string) {
	for _, p := range []string{prefixGame, prefixSet, prefixCard} {
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
	cards  []collection.Card             // every entry

	refreshing, backfilling bool

	// Tree data, rebuilt by applyFilter.
	filtered  []collection.Card
	setOrder  []string                       // set codes, newest release first
	setStats  map[string]collection.SetStats // per set, for the filtered cards
	setCards  map[string][]string            // set code -> card node IDs in collector-number order
	cardNodes map[string]collection.Card     // card node ID -> entry
	copies    int                            // copies in the filtered view
}

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
	v.searchEntry.OnChanged = func(s string) {
		v.query.Text = s
		ui.applyFilter()
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
	finish := newFilterSelect("Finish", titleCaseAll(g.Finishes()), func(s string) {
		v.query.Finish = s
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
		q := collection.Query{Text: v.query.Text, SortBy: collection.SortNumber, RarityOrder: gs.game.Rarities()}
		if gs == ui.g {
			q = v.query // menus come from the active game
		}
		gs.filtered = collection.ApplyQuery(gs.cards, q) // ordered by collector number

		st := collection.ComputeStats(gs.filtered, gs.sets)
		gs.setOrder = gs.setOrder[:0]
		gs.setStats = map[string]collection.SetStats{}
		for _, s := range st.BySet { // already newest first
			gs.setOrder = append(gs.setOrder, s.Code)
			gs.setStats[s.Code] = s
		}
		gs.setCards = map[string][]string{}
		gs.cardNodes = map[string]collection.Card{}
		gs.copies = st.Copies
		for _, c := range gs.filtered {
			code := strings.ToUpper(c.Set)
			id := cardNode(gs.game.ID(), c)
			gs.setCards[code] = append(gs.setCards[code], id)
			gs.cardNodes[id] = c
		}
	}

	if ui.tree != nil {
		ui.tree.Refresh()
		if v.filtering() {
			ui.tree.OpenAllBranches() // show every match without manual expanding
		} else {
			for _, gs := range ui.games {
				ui.tree.OpenBranch(gameNode(gs.game.ID()))
			}
		}
	}
	if v.summaryLabel != nil {
		gs := ui.g
		if v.filtering() {
			v.summaryLabel.SetText(fmt.Sprintf("%d of %s match · %s", len(gs.filtered), plural(len(gs.cards), "entry", "entries"), plural(gs.copies, "card", "cards")))
		} else {
			v.summaryLabel.SetText(fmt.Sprintf("%s · %s · %s", plural(len(gs.cards), "entry", "entries"), plural(gs.copies, "card", "cards"), plural(len(gs.setOrder), "set", "sets")))
		}
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

// buildTree creates the Home / game / set / card navigation tree.
func (ui *App) buildTree() *widget.Tree {
	tree := widget.NewTree(
		func(id widget.TreeNodeID) []widget.TreeNodeID {
			if id == "" {
				out := []string{nodeHome}
				for _, gs := range ui.games {
					out = append(out, gameNode(gs.game.ID()))
				}
				return out
			}
			kind, gameID, code := parseNode(id)
			gs := ui.gameByID(gameID)
			if gs == nil {
				return nil
			}
			switch kind {
			case prefixGame:
				out := make([]string, len(gs.setOrder))
				for i, c := range gs.setOrder {
					out[i] = setNode(gameID, c)
				}
				return out
			case prefixSet:
				return gs.setCards[code]
			}
			return nil
		},
		func(id widget.TreeNodeID) bool {
			kind, _, _ := parseNode(id)
			return id == "" || kind == prefixGame || kind == prefixSet
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

			kind, gameID, rest := parseNode(id)
			gs := ui.gameByID(gameID)
			switch {
			case id == nodeHome:
				icon.SetResource(theme.HomeIcon())
				name.SetText("Home")
				detail.SetText("")
			case gs == nil:
				name.SetText("")
				detail.SetText("")
			case kind == prefixGame:
				icon.SetResource(theme.StorageIcon())
				name.SetText(gs.game.Name())
				detail.SetText(plural(gs.copies, "card", "cards"))
			case kind == prefixSet:
				s := gs.setStats[rest]
				icon.SetResource(theme.FolderIcon())
				name.SetText(s.Name)
				if s.Name == "" {
					name.SetText(rest)
				}
				if s.SetSize > 0 {
					detail.SetText(fmt.Sprintf("%s · %d/%d", rest, s.UniqueNumbers, s.SetSize))
				} else {
					detail.SetText(fmt.Sprintf("%s · %d", rest, s.UniqueNumbers))
				}
			default:
				c := gs.cardNodes[id]
				icon.SetResource(nil)
				name.SetText(fmt.Sprintf("%s   %s", c.Number, c.Name))
				parts := []string{}
				if f := c.Finish; f != "" && len(gs.game.Finishes()) > 0 && f != gs.game.Finishes()[0] {
					parts = append(parts, f) // show non-default finishes only
				}
				if c.Condition != "" && c.Condition != collection.ConditionNearMint {
					parts = append(parts, string(c.Condition))
				}
				parts = append(parts, fmt.Sprintf("×%d", c.Quantity))
				detail.SetText(strings.Join(parts, " · "))
			}
			row.Refresh() // re-run the Border layout now that the labels have real widths
		},
	)

	tree.OnSelected = func(id widget.TreeNodeID) {
		ui.selected = id
		kind, gameID, rest := parseNode(id)
		if gs := ui.gameByID(gameID); gs != nil {
			ui.activate(gs)
		}
		switch {
		case id == nodeHome || kind == prefixGame:
			ui.showHome()
			ui.showDetail("Home")
		case kind == prefixSet:
			ui.showSet(rest)
			title := ui.g.setStats[rest].Name
			if title == "" {
				title = rest
			}
			ui.showDetail(title)
		case kind == prefixCard:
			if c, ok := ui.g.cardNodes[id]; ok {
				ui.renderEditor(c)
				ui.showDetail(c.Name)
			}
		}
	}
	return tree
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

// plural formats a count with the right noun, e.g. "1 entry", "3 entries".
func plural(n int, one, many string) string {
	if n == 1 {
		return fmt.Sprintf("%d %s", n, one)
	}
	return fmt.Sprintf("%d %s", n, many)
}
