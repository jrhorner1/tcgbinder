// Command mtgmanager is the trading card collection manager. By default it opens the
// desktop app; with --web it serves the same collection as a web application.
package main

import (
	"context"
	_ "embed"
	"errors"
	"flag"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"os"
	"os/exec"
	"os/signal"
	"path/filepath"
	"runtime"
	"syscall"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/app"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"
	"github.com/jrhorner1/mtgmanager/internal/datadir"
	"github.com/jrhorner1/mtgmanager/internal/game"
	"github.com/jrhorner1/mtgmanager/internal/games/mtg"
	"github.com/jrhorner1/mtgmanager/internal/instance"
	"github.com/jrhorner1/mtgmanager/internal/logging"
	"github.com/jrhorner1/mtgmanager/internal/service"
	"github.com/jrhorner1/mtgmanager/internal/ui"
	"github.com/jrhorner1/mtgmanager/internal/version"
	"github.com/jrhorner1/mtgmanager/internal/web"
)

const (
	appName = "MTG Manager"
	// appID must match FyneApp.toml. On iOS it is the bundle identifier, which has to
	// be registered to your Apple developer account before the app can be signed.
	appID = "io.github.jrhorner1.mtgmanager"
	// defaultListen keeps the web app private to this computer unless asked otherwise.
	defaultListen = "127.0.0.1:8080"
	logFileName   = "mtgmanager.log"
)

//go:embed Icon.png
var iconPNG []byte

// games lists the games the app manages. Each is its own package under
// internal/games; add new games here.
func games() []game.Game {
	return []game.Game{mtg.New()}
}

func main() {
	os.Exit(run(os.Args[1:], os.Stdout, os.Stderr))
}

type options struct {
	dataDir  string
	logFile  string
	logLevel string
	web      bool
	listen   string
	open     bool
	version  bool
}

func parseFlags(args []string, stdout, stderr io.Writer) (options, error) {
	var o options
	fs := flag.NewFlagSet("mtgmanager", flag.ContinueOnError)
	fs.SetOutput(stderr)
	fs.StringVar(&o.dataDir, "data-dir", os.Getenv("MTG_DATA_DIR"), "`folder` for collections and caches (default: the app's data folder; env MTG_DATA_DIR)")
	fs.StringVar(&o.logFile, "log-file", "", "write logs to `path` (default: "+logFileName+" in the data folder; \"-\" for stderr only)")
	fs.StringVar(&o.logLevel, "log-level", "info", "minimum log `level`: debug, info, warn or error")
	fs.BoolVar(&o.web, "web", false, "serve the app as a web application instead of opening the desktop window")
	fs.StringVar(&o.listen, "listen", defaultListen, "`address` for --web; use 0.0.0.0:8080 to allow other devices (no login!)")
	fs.BoolVar(&o.open, "open", false, "with --web, open the app in the default browser")
	fs.BoolVar(&o.version, "version", false, "print the version and exit")
	fs.Usage = func() {
		fmt.Fprintf(stderr, `%s — manage your trading card collection.

Usage:
  mtgmanager [flags]           open the desktop app
  mtgmanager --web [flags]     serve the app in a web browser

Flags (one or two dashes):
`, appName)
		fs.PrintDefaults()
		fmt.Fprintf(stderr, `
Examples:
  mtgmanager --web --open
  mtgmanager --web --listen 0.0.0.0:8080 --log-level debug
  mtgmanager --data-dir ~/cards --log-file ~/cards/app.log
`)
	}
	if err := fs.Parse(args); err != nil {
		return o, err
	}
	if fs.NArg() > 0 {
		fs.Usage()
		return o, fmt.Errorf("unexpected argument %q", fs.Arg(0))
	}
	if _, err := logging.ParseLevel(o.logLevel); err != nil {
		return o, err
	}
	return o, nil
}

func isMobile() bool { return runtime.GOOS == "ios" || runtime.GOOS == "android" }

func run(args []string, stdout, stderr io.Writer) int {
	opts, err := parseFlags(args, stdout, stderr)
	if errors.Is(err, flag.ErrHelp) {
		return 0
	}
	if err != nil {
		fmt.Fprintln(stderr, "mtgmanager:", err)
		return 2
	}
	if opts.version {
		fmt.Fprintln(stdout, version.String("mtgmanager"))
		return 0
	}

	// The desktop toolkit is only started for the desktop window (or on phones, where
	// it also provides the app's storage folder); --web runs without it.
	var a fyne.App
	if !opts.web {
		a = app.NewWithID(appID)
		a.SetIcon(fyne.NewStaticResource("Icon.png", iconPNG))
	}
	root, err := dataRoot(opts.dataDir, a)
	if err != nil {
		fmt.Fprintln(stderr, "mtgmanager: cannot find a data folder:", err)
		return 1
	}

	logFile := opts.logFile
	switch logFile {
	case "":
		logFile = filepath.Join(root, logFileName)
	case "-":
		logFile = ""
	}
	closeLog, err := logging.Setup(logging.Options{File: logFile, Level: opts.logLevel, Stderr: !isMobile()})
	if err != nil {
		fmt.Fprintln(stderr, "mtgmanager: cannot open log file:", err)
		return 1
	}
	defer closeLog()
	mode := "desktop"
	if opts.web {
		mode = "web"
	}
	slog.Info("starting", "version", version.String("mtgmanager"), "mode", mode, "data", root, "log", logFile)

	if !isMobile() {
		// Two copies writing the same files would overwrite each other's changes.
		lock, err := instance.Acquire(root)
		if errors.Is(err, instance.ErrRunning) {
			slog.Warn("already running; exiting", "err", err)
			if opts.web {
				fmt.Fprintf(stderr, "mtgmanager: %v (using %s)\n", err, root)
				return 1
			}
			showAlreadyRunning(a)
			return 1
		}
		if err != nil {
			slog.Warn("could not check for another running copy", "err", err)
		} else {
			defer lock.Release()
		}
	}

	var svcs []*service.Game
	for _, g := range games() {
		svc, err := service.Open(root, g, service.OpenOptions{
			LegacyFiles: g.ID() == mtg.ID, // Magic was the only game before multi-game support
			LegacyCWD:   g.ID() == mtg.ID && !isMobile(),
		})
		if err != nil {
			slog.Error("cannot open collection", "game", g.ID(), "err", err)
			fmt.Fprintf(stderr, "mtgmanager: cannot open %s collection: %v\n", g.Name(), err)
			return 1
		}
		svcs = append(svcs, svc)
	}

	if opts.web {
		return serveWeb(opts, svcs, stderr)
	}
	window := a.NewWindow(appName)
	window.Resize(fyne.NewSize(1200, 800))
	u := ui.New(window, svcs...)
	window.SetContent(u.BuildLayout())
	u.StartBackgroundSync()
	window.ShowAndRun()
	slog.Info("exiting")
	return 0
}

// serveWeb runs the web app until interrupted.
func serveWeb(opts options, svcs []*service.Game, stderr io.Writer) int {
	srv := web.New(appName, version.Version, svcs...)
	ln, url, err := srv.Listen(opts.listen)
	if err != nil {
		slog.Error("cannot listen", "addr", opts.listen, "err", err)
		fmt.Fprintf(stderr, "mtgmanager: cannot listen on %s: %v\n", opts.listen, err)
		return 1
	}
	httpSrv := srv.HTTPServer(ln)
	slog.Info("web app ready", "url", url)
	fmt.Fprintf(stderr, "%s is running at %s (Ctrl-C to stop)\n", appName, url)

	for _, svc := range svcs {
		go svc.DailySync(true) // load set sizes, price history and today's prices
		go func() {
			if _, err := svc.Sets(); err != nil {
				slog.Warn("could not load set list", "game", svc.Game.ID(), "err", err)
			}
		}()
	}
	if opts.open {
		openBrowser(url)
	}

	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	errc := make(chan error, 1)
	go func() { errc <- httpSrv.Serve(ln) }()
	select {
	case err := <-errc:
		if !errors.Is(err, http.ErrServerClosed) {
			slog.Error("web server stopped", "err", err)
			return 1
		}
	case <-ctx.Done():
		slog.Info("shutting down")
		shutdownCtx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
		defer cancel()
		_ = httpSrv.Shutdown(shutdownCtx)
	}
	return 0
}

func openBrowser(url string) {
	var cmd *exec.Cmd
	switch runtime.GOOS {
	case "darwin":
		cmd = exec.Command("open", url)
	case "windows":
		cmd = exec.Command("rundll32", "url.dll,FileProtocolHandler", url)
	default:
		cmd = exec.Command("xdg-open", url)
	}
	if err := cmd.Start(); err != nil {
		slog.Warn("could not open browser", "err", err)
	}
}

// dataRoot is the folder holding each game's folder: --data-dir / MTG_DATA_DIR if
// set, the app's storage on phones (the only writable place there), or the desktop
// data folder, which is the same one the desktop app has always used.
func dataRoot(flagDir string, a fyne.App) (string, error) {
	dir := flagDir
	if dir == "" && isMobile() && a != nil {
		if root := a.Storage().RootURI(); root != nil {
			dir = root.Path()
		}
	}
	if dir == "" {
		d, err := datadir.DefaultRoot(appID)
		if err != nil {
			return "", err
		}
		dir = d
	}
	if abs, err := filepath.Abs(dir); err == nil {
		dir = abs
	}
	return dir, os.MkdirAll(dir, 0o755)
}

// showAlreadyRunning tells the user a copy is already open, then quits.
func showAlreadyRunning(a fyne.App) {
	w := a.NewWindow(appName)
	msg := widget.NewLabel(appName + " is already running.\nSwitch to the open window to keep working.")
	msg.Alignment = fyne.TextAlignCenter
	ok := widget.NewButton("OK", a.Quit)
	ok.Importance = widget.HighImportance
	w.SetContent(container.NewPadded(container.NewVBox(msg, container.NewCenter(ok))))
	w.SetFixedSize(true)
	w.CenterOnScreen()
	w.SetCloseIntercept(a.Quit)
	w.ShowAndRun()
}
