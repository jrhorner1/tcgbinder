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
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/jrhorner1/mtgmanager/internal/collection"
	"github.com/jrhorner1/mtgmanager/internal/game"
	"github.com/jrhorner1/mtgmanager/internal/service"
)

//go:embed static
var staticFiles embed.FS

// ClientHeader must be sent with every request that changes data. Browsers won't let
// another website add a custom header to a cross-origin request without a CORS
// preflight, which this server never approves, so this blocks cross-site requests.
const ClientHeader = "X-MTG-Client"

// MaxUpload is the largest import file accepted.
const MaxUpload = 64 << 20

// Server is the web app for a set of games.
type Server struct {
	Name, Version string
	games         []*service.Game
	busy          sync.Map // "<game>/<job>" -> struct{}: long jobs running
	mux           *http.ServeMux
}

// New builds the server for games (already opened, each with its own files).
func New(name, version string, games ...*service.Game) *Server {
	s := &Server{Name: name, Version: version, games: games, mux: http.NewServeMux()}
	static, _ := fs.Sub(staticFiles, "static")
	s.mux.Handle("GET /", http.FileServerFS(static))
	s.mux.HandleFunc("GET /api/info", s.info)
	s.mux.HandleFunc("GET /api/games/{game}/overview", s.withGame(s.overview))
	s.mux.HandleFunc("GET /api/games/{game}/filters", s.withGame(s.filters))
	s.mux.HandleFunc("GET /api/games/{game}/cards", s.withGame(s.cards))
	s.mux.HandleFunc("POST /api/games/{game}/cards", s.withGame(s.addCard))
	s.mux.HandleFunc("GET /api/games/{game}/card", s.withGame(s.card))
	s.mux.HandleFunc("PUT /api/games/{game}/card", s.withGame(s.updateCard))
	s.mux.HandleFunc("DELETE /api/games/{game}/card", s.withGame(s.deleteCard))
	s.mux.HandleFunc("GET /api/games/{game}/search", s.withGame(s.search))
	s.mux.HandleFunc("POST /api/games/{game}/import", s.withGame(s.importFile))
	s.mux.HandleFunc("POST /api/games/{game}/refresh", s.withGame(s.refresh))
	s.mux.HandleFunc("POST /api/games/{game}/backfill", s.withGame(s.backfill))
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
	writeJSON(w, map[string]any{"name": s.Name, "version": s.Version, "games": games, "ranges": service.ChartRanges, "defaultRange": service.DefaultChartRange})
}

// cardView is a card as the browser sees it: the entry plus its key and value.
type cardView struct {
	collection.Card
	Key   string  `json:"key"`
	Each  float64 `json:"each,omitempty"`  // market price per copy
	Value float64 `json:"value,omitempty"` // each × quantity
}

func view(c collection.Card) cardView {
	v := cardView{Card: c, Key: collection.EntryKey(c)}
	if p, ok := collection.MarketPrice(c); ok {
		v.Each, v.Value = p, p*float64(c.Quantity)
	}
	return v
}

// MarshalJSON flattens the embedded card (which has its own MarshalJSON) with the
// extra fields.
func (v cardView) MarshalJSON() ([]byte, error) {
	b, err := json.Marshal(v.Card)
	if err != nil {
		return nil, err
	}
	var m map[string]any
	if err := json.Unmarshal(b, &m); err != nil {
		return nil, err
	}
	m["key"], m["each"], m["value"] = v.Key, v.Each, v.Value
	return json.Marshal(m)
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

func (s *Server) overview(w http.ResponseWriter, r *http.Request, g *service.Game) {
	cards := g.Store.GetCollection()
	st := collection.ComputeStats(cards, g.CachedSets())
	rng := r.URL.Query().Get("range")
	if rng == "" {
		rng = service.DefaultChartRange
	}
	vc := service.BuildValueChart(cards, st, g.Prices, rng, service.Today())

	type setRow struct {
		Code, Name, Released         string
		Owned, Size, Copies, Entries int
		Completion, Cost, Market     float64
	}
	sets := make([]setRow, 0, len(st.BySet))
	for _, s := range st.BySet {
		sets = append(sets, setRow{s.Code, s.Name, s.Released, s.UniqueNumbers, s.SetSize, s.Copies, s.Entries, s.Completion(), round2(s.Cost), round2(s.Market)})
	}
	top := make([]cardView, 0, len(st.TopCards))
	for _, cv := range st.TopCards {
		top = append(top, view(cv.Card))
	}
	src, canBackfill := g.CanBackfill()
	historyInfo := ""
	if canBackfill {
		historyInfo = src.PriceHistoryInfo()
	}
	writeJSON(w, map[string]any{
		"game": g.Game.ID(), "name": g.Game.Name(),
		"copies": st.Copies, "entries": st.Entries, "sets": st.Sets,
		"cost": round2(st.Cost), "costUnknown": st.CostUnknown,
		"market": round2(st.Market), "marketUnknown": st.MarketUnknown, "pricesAsOf": st.PricesAsOf,
		"bySet": sets, "topCards": top,
		"chart": map[string]any{
			"range": rng, "from": dateOrEmpty(vc.From), "to": dateOrEmpty(vc.To), "historyStart": dateOrEmpty(vc.HistoryStart),
			"market": points(vc.Market), "paid": points(vc.Paid), "estimate": points(vc.Estimate),
		},
		"canBackfill": canBackfill, "backfilled": g.Prices.BackfilledOn(), "historyInfo": historyInfo,
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
	writeJSON(w, map[string]any{
		"sets": sets, "rarities": g.Game.Rarities(), "finishes": g.Game.Finishes(), "filters": extra,
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

func (s *Server) cards(w http.ResponseWriter, r *http.Request, g *service.Game) {
	all := g.Store.GetCollection()
	filtered := collection.ApplyQuery(all, query(r, g.Game))
	st := collection.ComputeStats(filtered, g.CachedSets())
	type group struct {
		Code, Name          string
		Owned, Size, Copies int
		Cards               []cardView
	}
	groups := make([]*group, 0, len(st.BySet))
	byCode := map[string]*group{}
	for _, s := range st.BySet { // newest set first
		gr := &group{Code: s.Code, Name: s.Name, Owned: s.UniqueNumbers, Size: s.SetSize, Copies: s.Copies}
		groups = append(groups, gr)
		byCode[s.Code] = gr
	}
	for _, c := range filtered { // collector-number order
		if gr := byCode[strings.ToUpper(c.Set)]; gr != nil {
			gr.Cards = append(gr.Cards, view(c))
		}
	}
	writeJSON(w, map[string]any{"total": len(all), "matched": len(filtered), "copies": st.Copies, "sets": groups})
}

func (s *Server) findCard(g *service.Game, key string) (collection.Card, bool) {
	for _, c := range g.Store.GetCollection() {
		if collection.EntryKey(c) == key {
			return c, true
		}
	}
	return collection.Card{}, false
}

func (s *Server) card(w http.ResponseWriter, r *http.Request, g *service.Game) {
	c, ok := s.findCard(g, r.URL.Query().Get("key"))
	if !ok {
		httpError(w, http.StatusNotFound, errors.New("no such card"))
		return
	}
	if c.Text == "" { // fetch rules text once for older entries
		if typeLine, text, err := g.Game.CardText(c.ID); err == nil {
			g.Store.SetCatalogText(c.ID, typeLine, text)
			c.TypeLine, c.Text = typeLine, text
		}
	}
	others := []cardView{} // JSON lists are never null: the browser maps over them
	for _, o := range g.Store.GetCollection() {
		if o.Name == c.Name && collection.EntryKey(o) != collection.EntryKey(c) {
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
		"card": view(c), "details": details, "link": map[string]string{"label": label, "url": linkURL}, "others": others,
	})
}

// --- write endpoints -----------------------------------------------------------

type ownership struct {
	Quantity      int     `json:"quantity"`
	Finish        string  `json:"finish"`
	Condition     string  `json:"condition"`
	PurchasePrice float64 `json:"purchase_price"`
	AcquiredDate  string  `json:"acquired_date"`
}

func (o ownership) validate() error {
	if o.Quantity < 0 {
		return errors.New("quantity must be 0 or more")
	}
	if o.PurchasePrice < 0 {
		return errors.New("price paid can't be negative")
	}
	if o.AcquiredDate != "" {
		if _, err := time.Parse("2006-01-02", o.AcquiredDate); err != nil {
			return errors.New("acquired date must look like 2024-03-15")
		}
	}
	return nil
}

func (o ownership) apply(c *collection.Card) {
	c.Quantity = o.Quantity
	if o.Finish != "" {
		c.Finish = o.Finish
	}
	if o.Condition != "" {
		c.Condition = collection.ParseCondition(o.Condition)
	}
	c.PurchasePrice = o.PurchasePrice
	c.AcquiredDate = o.AcquiredDate
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
	var o ownership
	if err := decode(r, &o); err != nil {
		httpError(w, http.StatusBadRequest, err)
		return
	}
	if err := o.validate(); err != nil {
		httpError(w, http.StatusBadRequest, err)
		return
	}
	o.apply(&c)
	newKey, merged, err := g.Store.UpdateEntry(key, c)
	if err != nil {
		httpError(w, http.StatusInternalServerError, err)
		return
	}
	slog.Info("card updated", "game", g.Game.ID(), "card", c.Name, "key", newKey, "merged", merged)
	writeJSON(w, map[string]any{"key": newKey, "merged": merged, "deleted": c.Quantity == 0})
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

// addCard adds a catalog card (as returned by search) with ownership details.
func (s *Server) addCard(w http.ResponseWriter, r *http.Request, g *service.Game) {
	var req struct {
		Card collection.Card `json:"card"`
		ownership
	}
	if err := decode(r, &req); err != nil {
		httpError(w, http.StatusBadRequest, err)
		return
	}
	if req.Card.ID == "" || req.Quantity <= 0 {
		httpError(w, http.StatusBadRequest, errors.New("a card and a quantity of at least 1 are required"))
		return
	}
	if err := req.ownership.validate(); err != nil {
		httpError(w, http.StatusBadRequest, err)
		return
	}
	c := req.Card
	req.ownership.apply(&c)
	created, merged, err := g.Store.AddCards([]collection.Card{c})
	if err != nil {
		httpError(w, http.StatusInternalServerError, err)
		return
	}
	slog.Info("card added", "game", g.Game.ID(), "card", c.Name, "quantity", c.Quantity, "merged", merged > 0)
	writeJSON(w, map[string]any{"key": collection.EntryKey(c), "created": created, "merged": merged})
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
		return map[string]any{"summary": res.Summary(), "copies": res.Copies, "created": res.Created, "merged": res.Merged, "format": res.Report.Format}, nil
	})
}

func (s *Server) refresh(w http.ResponseWriter, r *http.Request, g *service.Game) {
	s.job(w, g.Game.ID()+"/refresh", func() (any, error) {
		n, err := g.RefreshPrices(nil)
		return map[string]int{"updated": n}, err
	})
}

func (s *Server) backfill(w http.ResponseWriter, r *http.Request, g *service.Game) {
	s.job(w, g.Game.ID()+"/backfill", func() (any, error) {
		n, err := g.Backfill(nil)
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
