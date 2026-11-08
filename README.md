# TCG Binder

A trading card game collection manager, starting with Magic: The Gathering. Cards are looked up on
[Scryfall](https://scryfall.com), the collection is stored locally as JSON, and
the app tracks what you paid, current market value, value over time, and set
completion. Collections can be imported from EchoMTG, ManaBox, Moxfield,
Deckbox, TCGplayer, Dragon Shield, Archidekt, or any CSV with recognizable
column names.

Built with Go and [Fyne](https://fyne.io), targeting macOS, Windows, Linux,
iOS, Android and the browser.

## Documentation

See [docs/](docs/README.md): user guide, web API, architecture, data sources and a
generated package reference.

## Usage

```
tcgbinder [flags]           open the desktop app
tcgbinder --web [flags]     serve the app in a web browser

  --data-dir folder    data folder (default: the app's data folder; env TCGBINDER_DATA_DIR)
  --log-file path      log file (default: tcgbinder.log in the data folder; "-" for stderr only)
  --log-level level    debug, info, warn or error (default info)
  --web                serve the web app instead of opening the desktop window
  --listen address     address for --web (default 127.0.0.1:8080)
  --open               with --web, open the app in the default browser
  --version            print the version and exit
  -h, --help           show help
```

Prices refresh automatically (at startup, then hourly checks refresh them once a day).

The web app offers the same collection as the desktop app (Home dashboard,
collection browser and editor, Add, Import, Export) and works on phone-sized
screens. It listens on this computer only by default. `--listen 0.0.0.0:8080`
makes it reachable from other devices on your network, but **it has no login**,
so anyone who can reach it can view and change your collection. Only one copy of
the app (desktop or web) can use a data folder at a time.

Logs are structured (`log/slog` text format); the log file is rotated to
`.1` at startup once it passes 10 MB.

## Layout

```
cmd/tcgbinder/                            app entry point; registers the games
internal/game/                             the Game interface every game implements
internal/games/mtg/                        Magic: The Gathering (package mtg)
internal/games/mtg/scryfall/               rate-limited Scryfall API client
internal/games/mtg/scryfall/scryfalltest/  fake Scryfall servers for tests
internal/games/mtg/importer/               CSV export parsing and matching rows to printings
internal/games/mtg/mtgjson/                MTGJSON price history download
internal/collection/                       game-neutral cards and their copies, JSON store, price history, stats
internal/datadir/                          data folder layout (one subfolder per game)
internal/imagecache/                       card image downloads with an in-memory cache
internal/instance/                         single-instance lock for the desktop app
internal/service/                          shared logic for one game: import, prices, history, daily sync
internal/ui/                               Fyne desktop/mobile UI; uses only the game interface
internal/web/                              --web: JSON API and embedded browser UI (static/)
internal/logging/                          log/slog setup, log file and level
internal/scan/                             card scanning: text read from a photo (game-neutral)
internal/scan/apple/                       Vision text reading (macOS/iOS) and the iOS camera scanner
internal/settings/                         user settings (theme, price updates, defaults), settings.json
internal/version/                          version string for --version
```

`collection` depends on nothing else in the app; each game builds on `collection`
and `game`; `ui` works only through `game.Game` and never imports a game.

### Adding a game

1. Create `internal/games/<id>/` with a type implementing `game.Game` (catalog
   search, price refresh, set sizes, CSV import, filters, editor details) and,
   optionally, `game.PriceHistorySource`.
2. Keep game-specific card fields in `collection.Card.Attrs` (`SetAttr`,
   `AttrString`); they are saved alongside the common fields.
3. Register it in `games()` in `cmd/tcgbinder/main.go`.

The game gets its own folder (`<data>/<id>/`) with its own `collection.json` and
`price-history.json`, and its own node in the collection tree.

## Develop

```bash
go run ./cmd/tcgbinder      # run the app
go test ./...                # unit and UI tests (offline)
go vet ./...
```

Benchmarks cover the hot paths (loading and saving, an edit, stats, filtering, the
value chart, price history, the web API and a search keystroke) at 500 and 20,000
cards:

```bash
go test -run '^$' -bench . -benchmem ./internal/collection ./internal/web ./internal/ui
```

Tests that call the live Scryfall API are skipped unless enabled:

```bash
MTG_LIVE=1 go test -run TestLiveSearch -v ./internal/games/mtg/scryfall
MTG_IMPORT_FILE=$PWD/export.csv go test -run TestLiveImport -v ./internal/games/mtg
MTG_IMPORT_FILE=$PWD/export.csv TCGBINDER_SNAPSHOT_DIR=/tmp go test -run TestDashboardSnapshots ./internal/ui
```

`TCGBINDER_SNAPSHOT_DIR` also makes the UI tests save PNG screenshots of each step.

## iPhone / iPad

iOS builds need the full Xcode app (the Command Line Tools alone are not enough)
and the [fyne CLI](https://docs.fyne.io/started/packaging):

```bash
go install fyne.io/tools/cmd/fyne@latest
sudo xcode-select -s /Applications/Xcode.app
xcodebuild -downloadPlatform iOS        # iOS SDK + simulator runtime
cd cmd/tcgbinder
../../scripts/build-ios-simulator.sh    # build for the iOS Simulator (no certificate needed)
../../scripts/build-ios-device.sh       # build for a device (needs signing)
```

Use `scripts/build-ios-device.sh` rather than a bare `fyne package -os ios`: it adds
the camera permission text card scanning needs (`fyne package` has no way to set
it, and iOS closes an app that opens the camera without it) and re-signs the app.

App metadata and the icon live in `cmd/tcgbinder/FyneApp.toml` and `Icon.png`.
`fyne package -os iossimulator` also needs a signing certificate, so the script
builds the Simulator app without one. If Anaconda is installed, make sure Apple's
`/usr/bin/codesign` comes first on your PATH (Anaconda ships a `codesign` that can't
sign apps).

The app ID (`io.github.jrhorner1.tcgbinder`) is the iOS bundle identifier; a
device build must be signed by an Apple ID set up in Xcode (a free account can
install on your own phone for 7 days at a time; the App Store needs the paid
Apple Developer Program).

On phones the app shows one screen at a time: the collection list, then the
dashboard, set or card page with a Back button.

## Price history

The Value Over Time chart uses real daily prices. MTGJSON's free
[AllPrices](https://mtgjson.com/downloads/all-files/) file supplies the last 90
days of TCGplayer prices (about a 50 MB download, matched to your cards through
MTGJSON's per-set files); after that, the app records Scryfall's prices each day
it is opened. Desktop loads the history automatically on first run; on phones it
waits for **Load 90-day price history** on the Home page. Before recorded prices
begin, a grey line estimates value at today's prices.

## Data

Each game keeps its own files in its own folder inside the app's private data
folder (`~/Library/Preferences/fyne/io.github.jrhorner1.tcgbinder/` on macOS,
the app sandbox on iOS/Android; set `TCGBINDER_DATA_DIR` to use another folder):

```
<data>/mtg/collection.json      Magic collection and daily value history
<data>/mtg/price-history.json   daily prices per printing, for the value chart
<data>/mtg/scryfall-sets.json   cached set sizes (refreshed weekly)
```

**Import** accepts CSV exports from other collection apps and collection `.json`
files written by this app (a backup or another device's collection). A JSON file
is checked against the game it belongs to; cards you already own gain the imported
purchases.

Only one copy of the desktop app can use a data folder at a time: a second launch
shows "already running" and quits. The lock (`tcgbinder.lock` in the data folder)
is an OS file lock, so it is released automatically even if the app crashes.
