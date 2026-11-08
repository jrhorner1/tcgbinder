# `internal/web`

```
package web // import "github.com/jrhorner1/tcgbinder/internal/web"

Package web serves the app as a web application: a JSON API over the same game
and collection services the desktop UI uses, plus a browser UI embedded in the
binary.

CONSTANTS

const ClientHeader = "X-TCGBinder-Client"
    ClientHeader must be sent with every request that changes data. Browsers
    won't let another website add a custom header to a cross-origin request
    without a CORS preflight, which this server never approves, so this blocks
    cross-site requests.

const MaxUpload = 64 << 20
    MaxUpload is the largest import file accepted.


TYPES

type Server struct {
	Name, Version string

	// Has unexported fields.
}
    Server is the web app for a set of games.

func New(name, version string, prefs *settings.Store, games ...*service.Game) *Server
    New builds the server for games (already opened, each with its own files),
    with the user's settings (nil for defaults that aren't saved).

func (s *Server) HTTPServer(ln net.Listener) *http.Server
    HTTPServer returns an http.Server for ln with sensible timeouts.

func (s *Server) Handler(ln net.Listener) http.Handler
    Handler returns the server's handler for a listener. On a loopback address
    only requests naming localhost or an IP are answered, which defeats DNS
    rebinding; on other addresses anyone who can reach the port can use the app.

func (s *Server) Listen(addr string) (net.Listener, string, error)
    Listen opens addr and returns the listener and the URL to open in a browser.

func (s *Server) ServeHTTP(w http.ResponseWriter, r *http.Request)
    ServeHTTP applies the request checks and routes the request.

```
