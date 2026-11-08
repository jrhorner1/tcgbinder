// Package web serves the app as a web application: a JSON API over the same game and
// collection services the desktop UI uses, plus a browser UI embedded in the binary.
package web

import (
	"embed"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"log/slog"
	"net"
	"net/http"
	"os"
	"path/filepath"
	"slices"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/jrhorner1/tcgbinder/internal/collection"
	"github.com/jrhorner1/tcgbinder/internal/game"
	"github.com/jrhorner1/tcgbinder/internal/service"
	"github.com/jrhorner1/tcgbinder/internal/settings"
)

//go:embed static
var staticFiles embed.FS

// ClientHeader must be sent with every request that changes data. Browsers won't let
// another website add a custom header to a cross-origin request without a CORS
// preflight, which this server never approves, so this blocks cross-site requests.
const ClientHeader = "X-TCGBinder-Client"

// MaxUpload is the largest import file accepted.
const MaxUpload = 64 << 20

// Server is the web app for a set of games.
type Server struct {
	Name, Version string
	settings      *settings.Store
	games         []*service.Game
	busy          sync.Map // "<game>/<job>" -> struct{}: long jobs running
	mux           *http.ServeMux
}

// New builds the server for games (already opened, each with its own files), with
// the user's settings (nil for defaults that aren't saved).
func New(name, version string, prefs *settings.Store, games ...*service.Game) *Server {
	if prefs == nil {
		prefs = settings.Memory()
	}
	s := &Server{Name: name, Version: version, settings: prefs, games: games, mux: http.NewServeMux()}
	static, _ := fs.Sub(staticFiles, "static")
	s.mux.Handle("GET /", http.FileServerFS(static))
	s.mux.HandleFunc("GET /api/info", s.info)
	s.mux.HandleFunc("GET /api/search", s.searchAll)
	s.mux.HandleFunc("GET /api/settings", s.getSettings)
	s.mux.HandleFunc("PUT /api/settings", s.putSettings)
	s.mux.HandleFunc("GET /api/games/{game}/set", s.withGame(s.setPage))
	s.mux.HandleFunc("GET /api/games/{game}/missing", s.withGame(s.missing))
	s.mux.HandleFunc("GET /api/games/{game}/printings", s.withGame(s.printings))
	s.mux.HandleFunc("PUT /api/games/{game}/card/printing", s.withGame(s.changePrinting))
	s.mux.HandleFunc("POST /api/games/{game}/bulk", s.withGame(s.bulkEdit))
	s.mux.HandleFunc("GET /api/games/{game}/home", s.withGame(s.home))
	s.mux.HandleFunc("GET /api/games/{game}/filters", s.withGame(s.filters))
	s.mux.HandleFunc("GET /api/games/{game}/cards", s.withGame(s.cards))
	s.mux.HandleFunc("POST /api/games/{game}/cards", s.withGame(s.addCard))
	s.mux.HandleFunc("GET /api/games/{game}/card", s.withGame(s.card))
	s.mux.HandleFunc("PUT /api/games/{game}/card", s.withGame(s.updateCard))
	s.mux.HandleFunc("DELETE /api/games/{game}/card", s.withGame(s.deleteCard))
	s.mux.HandleFunc("GET /api/games/{game}/search", s.withGame(s.search))
	s.mux.HandleFunc("POST /api/games/{game}/import", s.withGame(s.importFile))
	s.mux.HandleFunc("POST /api/games/{game}/refresh", s.withGame(s.refresh))
	s.mux.HandleFunc("POST /api/games/{game}/price-history", s.withGame(s.loadPriceHistory))
	s.mux.HandleFunc("GET /api/games/{game}/export", s.withGame(s.export))
	return s
}

// ServeHTTP applies the request checks and routes the request.
func (s *Server) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet && r.Method != http.MethodHead && r.Header.Get(ClientHeader) == "" {
		httpError(w, http.StatusForbidden, errors.New("missing "+ClientHeader+" header"))
		return
	}
	w.Header().Set("X-Content-Type-Options", "nosniff")
	w.Header().Set("Referrer-Policy", "no-referrer")
	start := time.Now()
	rec := &statusRecorder{ResponseWriter: w, status: 200}
	s.mux.ServeHTTP(rec, r)
	if strings.HasPrefix(r.URL.Path, "/api/") {
		slog.Debug("http", "method", r.Method, "path", r.URL.Path, "status", rec.status, "duration", time.Since(start).Round(time.Millisecond))
	}
}

type statusRecorder struct {
	http.ResponseWriter
	status int
}

func (r *statusRecorder) WriteHeader(code int) {
	r.status = code
	r.ResponseWriter.WriteHeader(code)
}

// Listen opens addr and returns the listener and the URL to open in a browser.
func (s *Server) Listen(addr string) (net.Listener, string, error) {
	ln, err := net.Listen("tcp", addr)
	if err != nil {
		return nil, "", err
	}
	return ln, "http://" + displayAddr(ln.Addr()) + "/", nil
}

// Handler returns the server's handler for a listener. On a loopback address only
// requests naming localhost or an IP are answered, which defeats DNS rebinding; on
// other addresses anyone who can reach the port can use the app.
func (s *Server) Handler(ln net.Listener) http.Handler {
	if isLoopback(ln.Addr()) {
		return loopbackOnly(s)
	}
	slog.Warn("web app is reachable from other computers and has no login: anyone who can reach it can view and change your collection", "addr", ln.Addr().String())
	return s
}

// HTTPServer returns an http.Server for ln with sensible timeouts.
func (s *Server) HTTPServer(ln net.Listener) *http.Server {
	return &http.Server{Handler: s.Handler(ln), ReadHeaderTimeout: 10 * time.Second}
}

func isLoopback(a net.Addr) bool {
	if tcp, ok := a.(*net.TCPAddr); ok {
		return tcp.IP.IsLoopback()
	}
	return false
}

func displayAddr(a net.Addr) string {
	if tcp, ok := a.(*net.TCPAddr); ok && tcp.IP.IsLoopback() {
		return fmt.Sprintf("localhost:%d", tcp.Port)
	}
	return a.String()
}

// loopbackOnly rejects requests whose Host isn't localhost or an IP address.
func loopbackOnly(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		host := r.Host
		if h, _, err := net.SplitHostPort(r.Host); err == nil {
			host = h
		}
		if host != "localhost" && net.ParseIP(host) == nil {
			httpError(w, http.StatusForbidden, fmt.Errorf("unexpected host %q", r.Host))
			return
		}
		next.ServeHTTP(w, r)
	})
}

// --- helpers -------------------------------------------------------------------

func writeJSON(w http.ResponseWriter, v any) {
	w.Header().Set("Content-Type", "application/json; charset=utf-8")
	w.Header().Set("Cache-Control", "no-store")
	if err := json.NewEncoder(w).Encode(v); err != nil {
		slog.Warn("writing response", "err", err)
	}
}

func httpError(w http.ResponseWriter, status int, err error) {
	w.Header().Set("Content-Type", "application/json; charset=utf-8")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(map[string]string{"error": err.Error()})
}

func (s *Server) withGame(h func(http.ResponseWriter, *http.Request, *service.Game)) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		id := r.PathValue("game")
		for _, g := range s.games {
			if g.Game.ID() == id {
				h(w, r, g)
				return
			}
		}
		httpError(w, http.StatusNotFound, fmt.Errorf("unknown game %q", id))
	}
}

// job runs a long operation unless one of the same kind is already running.
func (s *Server) job(w http.ResponseWriter, key string, run func() (any, error)) {
	if _, running := s.busy.LoadOrStore(key, struct{}{}); running {
		httpError(w, http.StatusConflict, errors.New("already running"))
		return
	}
	defer s.busy.Delete(key)
	v, err := run()
	if err != nil {
		httpError(w, http.StatusBadGateway, err)
		return
	}
	writeJSON(w, v)
}

// --- read endpoints ------------------------------------------------------------

func (s *Server) info(w http.ResponseWriter, r *http.Request) {
	type gameInfo struct {
		ID, Name string
	}
	var games []gameInfo
	for _, g := range s.games {
		games = append(games, gameInfo{g.Game.ID(), g.Game.Name()})
	}
	writeJSON(w, map[string]any{"name": s.Name, "version": s.Version, "games": games, "ranges": service.ChartRanges})
}

// cardView is a card as the browser sees it: the card plus its key, totals and its
// purchases as a flat list (finish, condition, quantity, price paid, date), as the
// "Your Copies" rows show them.
type cardView struct {
	collection.Card
	Key         string         `json:"key"`
	CopyCount   int            `json:"copy_count"`   // copies owned
	Each        float64        `json:"each"`         // market price of one copy in its most valuable owned finish
	MarketValue float64        `json:"market_value"` // market value of every copy
	PricePaid   float64        `json:"price_paid"`   // price paid for every copy with a known price
	Purchases   []purchaseView `json:"purchases"`
}

// purchaseView is one purchase of a card in a finish and condition.
type purchaseView struct {
	Finish       string  `json:"finish"`
	Condition    string  `json:"condition"`
	Quantity     int     `json:"quantity"`
	PricePaid    float64 `json:"price_paid"` // per copy
	AcquiredDate string  `json:"acquired_date"`
	Market       float64 `json:"market"` // market price of one copy in this finish (0 if unknown)
}

func view(c collection.Card) cardView {
	v := cardView{Card: c, Key: collection.CardKey(c), CopyCount: c.CopyCount(), Purchases: []purchaseView{}}
	each, _ := c.BestPrice()
	value, _ := c.MarketValue()
	paid, _ := c.PricePaid()
	v.Each, v.MarketValue, v.PricePaid = round2(each), round2(value), round2(paid)
	for _, p := range c.PurchaseRows() {
		m, _ := collection.MarketPrice(c, p.Finish)
		v.Purchases = append(v.Purchases, purchaseView{p.Finish, string(p.Condition), p.Quantity, p.PricePaid, p.AcquiredDate, m})
	}
	return v
}

type point struct {
	Date  string  `json:"date"`
	Value float64 `json:"value"`
}

func points(ps []collection.TimePoint) []point {
	out := make([]point, 0, len(ps))
	for _, p := range ps {
		out = append(out, point{p.Date.Format("2006-01-02"), round2(p.Value)})
	}
	return out
}

func round2(v float64) float64 { return float64(int64(v*100+0.5)) / 100 }

func dateOrEmpty(t time.Time) string {
	if t.IsZero() {
		return ""
	}
	return t.Format("2006-01-02")
}

func (s *Server) home(w http.ResponseWriter, r *http.Request, g *service.Game) {
	cards := g.Store.GetCollection()
	st := collection.ComputeStats(cards, g.CachedSets())
	rng := r.URL.Query().Get("range")
	if rng == "" {
		rng = service.DefaultChartRange
	}
	vc := service.BuildValueChart(cards, st, g.Prices, rng, service.Today())

	type setRow struct {
		Code, Name, Released                string
		Collected, Size, Cards, UniqueCards int
		Completion, PricePaid, MarketValue  float64
	}
	sets := make([]setRow, 0, len(st.BySet))
	for _, s := range st.BySet {
		sets = append(sets, setRow{s.Code, s.Name, s.Released, s.Collected, s.SetSize, s.Cards, s.UniqueCards, s.Completion(), round2(s.PricePaid), round2(s.MarketValue)})
	}
	top := make([]cardView, 0, len(st.TopCards))
	for _, cv := range st.TopCards {
		top = append(top, view(cv.Card))
	}
	src, canLoadHistory := g.PriceHistorySource()
	historyInfo := ""
	if canLoadHistory {
		historyInfo = src.PriceHistoryInfo()
	}
	writeJSON(w, map[string]any{
		"game": g.Game.ID(), "name": g.Game.Name(),
		"cards": st.Cards, "uniqueCards": st.UniqueCards, "sets": st.Sets,
		"pricePaid": round2(st.PricePaid), "pricePaidUnknown": st.PricePaidUnknown,
		"marketValue": round2(st.MarketValue), "unpriced": st.Unpriced, "pricesAsOf": st.PricesAsOf,
		"bySet": sets, "topCards": top,
		"chart": map[string]any{
			"range": rng, "from": dateOrEmpty(vc.From), "to": dateOrEmpty(vc.To), "historyStart": dateOrEmpty(vc.HistoryStart),
			"marketValue": points(vc.MarketValue), "pricePaid": points(vc.PricePaid), "estimate": points(vc.Estimate),
		},
		"canLoadHistory": canLoadHistory, "historyLoaded": g.Prices.HistoryLoadedOn(), "historyInfo": historyInfo,
	})
}

func (s *Server) filters(w http.ResponseWriter, r *http.Request, g *service.Game) {
	labels, byLabel := collection.SetOptions(g.Store.GetCollection())
	type opt struct{ Code, Label string }
	sets := []opt{}
	for _, l := range labels {
		if code := byLabel[l]; code != "" {
			sets = append(sets, opt{code, l})
		}
	}
	type filter struct {
		Name    string
		Options []string
	}
	extra := []filter{}
	for _, f := range g.Game.Filters() {
		extra = append(extra, filter{f.Name, f.Options})
	}
	text, set := g.Game.SearchHints()
	finishNames := map[string]string{}
	for _, f := range g.Game.Finishes() {
		finishNames[f] = g.Game.FinishName(f)
	}
	writeJSON(w, map[string]any{
		"sets": sets, "rarities": g.Game.Rarities(), "finishes": g.Game.Finishes(), "finishNames": finishNames, "filters": extra,
		"conditions": []string{"NM", "LP", "MP", "HP", "DMG"}, "searchHints": []string{text, set},
	})
}

// query builds a collection query from URL parameters: q, set, rarity, finish and
// one parameter per game filter, named f.<filter name>.
func query(r *http.Request, g game.Game) collection.Query {
	v := r.URL.Query()
	q := collection.Query{Text: v.Get("q"), Set: v.Get("set"), Rarity: v.Get("rarity"), Finish: v.Get("finish"),
		RarityOrder: g.Rarities(), SortBy: collection.SortNumber}
	filters := g.Filters()
	q.Match = func(c collection.Card) bool {
		for _, f := range filters {
			if opt := v.Get("f." + f.Name); !collection.IsAny(opt) && !f.Match(c, opt) {
				return false
			}
		}
		return true
	}
	return q
}

// cards lists the filtered cards in the Card order setting's order, with counts.
func (s *Server) cards(w http.ResponseWriter, r *http.Request, g *service.Game) {
	all := g.Store.GetCollection()
	filtered := collection.SortCards(collection.ApplyQuery(all, query(r, g.Game)), s.settings.Get().CardOrder)
	views := make([]cardView, len(filtered))
	sets := map[string]bool{}
	cardCount := 0
	for i, c := range filtered {
		views[i] = view(c)
		cardCount += views[i].CopyCount
		sets[strings.ToUpper(c.Set)] = true
	}
	writeJSON(w, map[string]any{"total": len(all), "matched": len(filtered), "cardCount": cardCount,
		"uniqueCards": collection.UniqueCards(filtered), "sets": len(sets), "cards": views})
}

func (s *Server) findCard(g *service.Game, key string) (collection.Card, bool) {
	return g.Store.Get(key)
}

func (s *Server) card(w http.ResponseWriter, r *http.Request, g *service.Game) {
	c, ok := s.findCard(g, r.URL.Query().Get("key"))
	if !ok {
		httpError(w, http.StatusNotFound, errors.New("no such card"))
		return
	}
	others := []cardView{} // JSON lists are never null: the browser maps over them
	for _, o := range g.Store.GetCollection() {
		if o.Name == c.Name && collection.CardKey(o) != collection.CardKey(c) {
			others = append(others, view(o))
		}
	}
	sort.Slice(others, func(i, j int) bool { return others[i].ReleasedAt > others[j].ReleasedAt })
	label, link := g.Game.CardLink(c)
	linkURL := ""
	if link != nil {
		linkURL = link.String()
	}
	details := g.Game.Details(c)
	if details == nil {
		details = []game.Row{} // e.g. tokens have no mana cost
	}
	writeJSON(w, map[string]any{
		"card": view(c), "raw": c, "details": details, "link": map[string]string{"label": label, "url": linkURL}, "others": others,
	})
}

// --- write endpoints -----------------------------------------------------------

// purchaseForm is one purchase sent by the browser: copies in a finish and condition.
type purchaseForm struct {
	Quantity     int     `json:"quantity"`
	Finish       string  `json:"finish"`
	Condition    string  `json:"condition"`
	PricePaid    float64 `json:"price_paid"` // per copy
	AcquiredDate string  `json:"acquired_date"`
}

func (o purchaseForm) validate(c collection.Card) error {
	if o.Finish != "" && len(c.Finishes) > 0 && !slices.Contains(c.Finishes, o.Finish) {
		return fmt.Errorf("this printing doesn't come in %q", o.Finish)
	}
	if o.Quantity < 0 {
		return errors.New("quantity must be 0 or more")
	}
	if o.PricePaid < 0 {
		return errors.New("price paid can't be negative")
	}
	if o.AcquiredDate != "" {
		if _, err := time.Parse("2006-01-02", o.AcquiredDate); err != nil {
			return errors.New("acquired date must look like 2024-03-15")
		}
	}
	return nil
}

// add adds the purchase to c (in the printing's first finish if none is given).
func (o purchaseForm) add(c *collection.Card) {
	finish := o.Finish
	if finish == "" && len(c.Finishes) > 0 {
		finish = c.Finishes[0]
	}
	c.AddPurchase(finish, collection.ParseCondition(o.Condition), collection.Purchase{Quantity: o.Quantity, PricePaid: o.PricePaid, AcquiredDate: o.AcquiredDate})
}

func decode(r *http.Request, v any) error {
	return json.NewDecoder(io.LimitReader(r.Body, 1<<20)).Decode(v)
}

func (s *Server) updateCard(w http.ResponseWriter, r *http.Request, g *service.Game) {
	key := r.URL.Query().Get("key")
	c, ok := s.findCard(g, key)
	if !ok {
		httpError(w, http.StatusNotFound, errors.New("no such card"))
		return
	}
	var req struct {
		Purchases []purchaseForm `json:"purchases"` // every purchase of the card: replaces its copies
	}
	if err := decode(r, &req); err != nil {
		httpError(w, http.StatusBadRequest, err)
		return
	}
	c.Copies = nil
	for _, o := range req.Purchases {
		if err := o.validate(c); err != nil {
			httpError(w, http.StatusBadRequest, err)
			return
		}
		o.add(&c)
	}
	newKey, merged, err := g.Store.SaveCard(key, c)
	if err != nil {
		httpError(w, http.StatusInternalServerError, err)
		return
	}
	slog.Info("card updated", "game", g.Game.ID(), "card", c.Name, "key", newKey, "copies", c.CopyCount())
	writeJSON(w, map[string]any{"key": newKey, "merged": merged, "deleted": c.CopyCount() == 0})
}

func (s *Server) deleteCard(w http.ResponseWriter, r *http.Request, g *service.Game) {
	key := r.URL.Query().Get("key")
	c, ok := s.findCard(g, key)
	if !ok {
		httpError(w, http.StatusNotFound, errors.New("no such card"))
		return
	}
	if err := g.Store.RemoveCard(key); err != nil {
		httpError(w, http.StatusInternalServerError, err)
		return
	}
	slog.Info("card deleted", "game", g.Game.ID(), "card", c.Name, "key", key)
	writeJSON(w, map[string]any{"deleted": key})
}

func (s *Server) search(w http.ResponseWriter, r *http.Request, g *service.Game) {
	v := r.URL.Query()
	page, err := g.Game.Search(v.Get("text"), v.Get("set"), v.Get("next"))
	if err != nil {
		httpError(w, http.StatusBadGateway, err)
		return
	}
	results := make([]cardView, 0, len(page.Cards))
	for _, c := range page.Cards {
		results = append(results, view(c))
	}
	writeJSON(w, map[string]any{"cards": results, "total": page.Total, "next": page.NextPage})
}

// addCard adds a catalog card (as returned by search) with purchaseForm details.
func (s *Server) addCard(w http.ResponseWriter, r *http.Request, g *service.Game) {
	var req struct {
		Card collection.Card `json:"card"`
		purchaseForm
	}
	if err := decode(r, &req); err != nil {
		httpError(w, http.StatusBadRequest, err)
		return
	}
	if req.Card.ID == "" || req.Quantity <= 0 {
		httpError(w, http.StatusBadRequest, errors.New("a card and a quantity of at least 1 are required"))
		return
	}
	if err := req.purchaseForm.validate(req.Card); err != nil {
		httpError(w, http.StatusBadRequest, err)
		return
	}
	c := req.Card
	c.Copies = nil // only the purchase sent is added
	req.purchaseForm.add(&c)
	newCards, alreadyOwned, err := g.Store.AddCards([]collection.Card{c})
	if err != nil {
		httpError(w, http.StatusInternalServerError, err)
		return
	}
	slog.Info("card added", "game", g.Game.ID(), "card", c.Name, "copies", c.CopyCount(), "already_owned", alreadyOwned > 0)
	writeJSON(w, map[string]any{"key": collection.CardKey(c), "newCards": newCards, "alreadyOwned": alreadyOwned})
}

// importFile accepts the file as a multipart "file" field or as the raw body.
func (s *Server) importFile(w http.ResponseWriter, r *http.Request, g *service.Game) {
	r.Body = http.MaxBytesReader(w, r.Body, MaxUpload)
	var data []byte
	var err error
	if strings.HasPrefix(r.Header.Get("Content-Type"), "multipart/") {
		f, _, ferr := r.FormFile("file")
		if ferr != nil {
			httpError(w, http.StatusBadRequest, ferr)
			return
		}
		defer f.Close()
		data, err = io.ReadAll(f)
	} else {
		data, err = io.ReadAll(r.Body)
	}
	if err != nil {
		httpError(w, http.StatusBadRequest, err)
		return
	}
	s.job(w, g.Game.ID()+"/import", func() (any, error) {
		res, err := g.Import(data, nil)
		if err != nil {
			return nil, err
		}
		return map[string]any{"summary": res.Summary(), "cards": res.Cards, "newCards": res.NewCards, "alreadyOwned": res.AlreadyOwned, "format": res.Report.Format}, nil
	})
}

func (s *Server) refresh(w http.ResponseWriter, r *http.Request, g *service.Game) {
	s.job(w, g.Game.ID()+"/refresh", func() (any, error) {
		n, err := g.RefreshPrices(nil)
		return map[string]int{"updated": n}, err
	})
}

func (s *Server) loadPriceHistory(w http.ResponseWriter, r *http.Request, g *service.Game) {
	s.job(w, g.Game.ID()+"/price-history", func() (any, error) {
		n, err := g.LoadPriceHistory(nil)
		return map[string]int{"printings": n}, err
	})
}

// export downloads the game's collection file, which Import accepts.
func (s *Server) export(w http.ResponseWriter, r *http.Request, g *service.Game) {
	data, err := os.ReadFile(g.Store.FilePath())
	if err != nil {
		httpError(w, http.StatusInternalServerError, err)
		return
	}
	name := fmt.Sprintf("%s-collection-%s.json", g.Game.ID(), time.Now().Format("2006-01-02"))
	w.Header().Set("Content-Type", "application/json")
	w.Header().Set("Content-Disposition", `attachment; filename="`+name+`"`)
	w.Header().Set("Content-Length", strconv.Itoa(len(data)))
	_, _ = w.Write(data)
}

// --- set page, missing cards, printings, bulk edit, search ----------------------

func cardViews(cards []collection.Card) []cardView {
	out := make([]cardView, 0, len(cards))
	for _, c := range cards {
		out = append(out, view(c))
	}
	return out
}

func (s *Server) setPage(w http.ResponseWriter, r *http.Request, g *service.Game) {
	code := strings.ToUpper(r.URL.Query().Get("code"))
	var cards []collection.Card
	for _, c := range g.Store.GetCollection() {
		if strings.EqualFold(c.Set, code) {
			cards = append(cards, c)
		}
	}
	st := collection.ComputeStats(cards, g.CachedSets())
	if len(st.BySet) == 0 {
		httpError(w, http.StatusNotFound, fmt.Errorf("no cards from set %q", code))
		return
	}
	set := st.BySet[0]
	top := []cardView{}
	for _, cv := range collection.TopByEach(cards, 10) { // ranked by one copy
		top = append(top, view(cv.Card))
	}
	writeJSON(w, map[string]any{
		"code": set.Code, "name": set.Name, "released": set.Released,
		"collected": set.Collected, "size": set.SetSize, "completion": set.Completion(),
		"cards": set.Cards, "uniqueCards": set.UniqueCards,
		"pricePaid": round2(set.PricePaid), "marketValue": round2(set.MarketValue), "top": top,
	})
}

func (s *Server) missing(w http.ResponseWriter, r *http.Request, g *service.Game) {
	code := strings.ToUpper(r.URL.Query().Get("set"))
	setCards, err := g.Game.SetCards(code)
	if err != nil {
		httpError(w, http.StatusBadGateway, err)
		return
	}
	var owned []collection.Card
	for _, c := range g.Store.GetCollection() {
		if strings.EqualFold(c.Set, code) {
			owned = append(owned, c)
		}
	}
	writeJSON(w, map[string]any{"set": code, "total": len(setCards), "missing": cardViews(collection.Missing(setCards, owned))})
}

func (s *Server) printings(w http.ResponseWriter, r *http.Request, g *service.Game) {
	c, ok := s.findCard(g, r.URL.Query().Get("key"))
	if !ok {
		httpError(w, http.StatusNotFound, errors.New("no such card"))
		return
	}
	found, err := g.Game.Printings(c)
	if err != nil {
		httpError(w, http.StatusBadGateway, err)
		return
	}
	writeJSON(w, map[string]any{"current": c.ID, "printings": cardViews(found)})
}

// changePrinting moves a card to another printing of the same card. The printing is
// looked up again on the server rather than trusted from the request.
func (s *Server) changePrinting(w http.ResponseWriter, r *http.Request, g *service.Game) {
	key := r.URL.Query().Get("key")
	c, ok := s.findCard(g, key)
	if !ok {
		httpError(w, http.StatusNotFound, errors.New("no such card"))
		return
	}
	var req struct {
		ID string `json:"id"`
	}
	if err := decode(r, &req); err != nil || req.ID == "" {
		httpError(w, http.StatusBadRequest, errors.New("the printing's id is required"))
		return
	}
	found, err := g.Game.Printings(c)
	if err != nil {
		httpError(w, http.StatusBadGateway, err)
		return
	}
	for _, p := range found {
		if p.ID != req.ID {
			continue
		}
		moved := collection.WithPrinting(c, p)
		newKey, merged, err := g.Store.SaveCard(key, moved)
		if err != nil {
			httpError(w, http.StatusInternalServerError, err)
			return
		}
		slog.Info("printing changed", "game", g.Game.ID(), "card", c.Name, "from", c.Set+" "+c.CollectorNumber, "to", moved.Set+" "+moved.CollectorNumber)
		finishChanged := !slices.Equal(moved.OwnedFinishes(), c.OwnedFinishes())
		writeJSON(w, map[string]any{"key": newKey, "merged": merged, "finishes": moved.OwnedFinishes(), "finishChanged": finishChanged})
		return
	}
	httpError(w, http.StatusBadRequest, errors.New("that isn't a printing of this card"))
}

func (s *Server) bulkEdit(w http.ResponseWriter, r *http.Request, g *service.Game) {
	var req struct {
		Keys         []string `json:"keys"`
		Condition    string   `json:"condition"`
		Finish       string   `json:"finish"`
		PricePaid    *float64 `json:"price_paid"`
		AcquiredDate string   `json:"acquired_date"`
		Delete       bool     `json:"delete"`
	}
	if err := decode(r, &req); err != nil || len(req.Keys) == 0 {
		httpError(w, http.StatusBadRequest, errors.New("choose at least one card"))
		return
	}
	var change collection.BulkChange
	if req.Condition != "" {
		c := collection.ParseCondition(req.Condition)
		change.Condition = &c
	}
	if req.Finish != "" {
		change.Finish = &req.Finish
	}
	if req.PricePaid != nil {
		if *req.PricePaid < 0 {
			httpError(w, http.StatusBadRequest, errors.New("price paid can't be negative"))
			return
		}
		change.PricePaid = req.PricePaid
	}
	if req.AcquiredDate != "" {
		if _, err := time.Parse("2006-01-02", req.AcquiredDate); err != nil {
			httpError(w, http.StatusBadRequest, errors.New("acquired date must look like 2024-03-15"))
			return
		}
		change.AcquiredDate = &req.AcquiredDate
	}
	change.Delete = req.Delete
	if !change.Delete && change.Condition == nil && change.Finish == nil && change.PricePaid == nil && change.AcquiredDate == nil {
		httpError(w, http.StatusBadRequest, errors.New("choose at least one change"))
		return
	}
	res, err := g.Store.BulkEdit(req.Keys, change)
	if err != nil {
		httpError(w, http.StatusInternalServerError, err)
		return
	}
	if res.Skipped == nil {
		res.Skipped = []string{}
	}
	slog.Info("bulk edit", "game", g.Game.ID(), "cards", len(req.Keys), "updated", res.Updated, "deleted", res.Deleted)
	writeJSON(w, res)
}

// searchAll searches every game's collection.
func (s *Server) searchAll(w http.ResponseWriter, r *http.Request) {
	q := strings.TrimSpace(r.URL.Query().Get("q"))
	type result struct {
		Game, Name string
		Total      int
		Cards      []cardView
	}
	out := []result{}
	for _, g := range s.games {
		matches := collection.ApplyQuery(g.Store.GetCollection(), collection.Query{Text: q, SortBy: collection.SortName})
		res := result{Game: g.Game.ID(), Name: g.Game.Name(), Total: len(matches)}
		if len(matches) > 200 {
			matches = matches[:200]
		}
		res.Cards = cardViews(matches)
		out = append(out, res)
	}
	writeJSON(w, map[string]any{"q": q, "games": out})
}

// --- settings --------------------------------------------------------------------

// settingsChoices lists each setting's allowed values, for the settings page.
func settingsChoices() map[string][]string {
	return map[string][]string{
		"theme": settings.Themes, "text_size": settings.TextSizes, "card_order": settings.CardOrders,
		"chart_range": settings.ChartRanges, "price_refresh": settings.PriceRefreshes,
		"default_condition": settings.Conditions, "default_date": settings.DefaultDates, "log_level": settings.LogLevels,
	}
}

func (s *Server) getSettings(w http.ResponseWriter, r *http.Request) {
	dataDir := ""
	if p := s.settings.Path(); p != "" {
		dataDir = filepath.Dir(p)
	}
	writeJSON(w, map[string]any{"settings": s.settings.Get(), "choices": settingsChoices(),
		"defaults": settings.Defaults(), "dataDir": dataDir})
}

// putSettings changes the settings sent; settings left out keep their values.
func (s *Server) putSettings(w http.ResponseWriter, r *http.Request) {
	next := s.settings.Get()
	if err := decode(r, &next); err != nil {
		httpError(w, http.StatusBadRequest, err)
		return
	}
	if err := s.settings.Set(next); err != nil {
		httpError(w, http.StatusBadRequest, err)
		return
	}
	writeJSON(w, map[string]any{"settings": s.settings.Get()})
}
