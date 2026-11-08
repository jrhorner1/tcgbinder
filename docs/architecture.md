# Architecture

```
cmd/tcgbinder          flags, logging, data folder, single-instance lock; registers games;
                        runs the Fyne UI or the web server
internal/ui             Fyne desktop/phone UI          ┐ both use service and the
internal/web            web server + browser UI       ┘ game interface only
internal/service        one game's collection: open, import, prices, history, sync
internal/game           the Game interface each game implements
internal/games/mtg      Magic: Scryfall catalog, CSV importer, MTGJSON price history
internal/collection     game-neutral cards and their copies, JSON store, price history, stats, filters, bulk edit
internal/datadir        data folder layout
internal/imagecache     card image downloads (in-memory cache)
internal/instance       single-instance file lock
internal/logging        log/slog setup (level adjustable at runtime)
internal/scan           card scanning: text read from a photo, with positions (game-neutral)
internal/scan/apple     Vision text reading (macOS/iOS, cgo + Objective-C) and the iOS camera scanner
internal/settings       user settings (settings.json), shared by the UIs and services
internal/version        version string
```

**Dependency direction:** `collection` ← `game` ← `games/*` ← `service` ← `ui`/`web` ← `cmd`;
`settings` depends on nothing in the app and is used by `service`, `ui`, `web` and `cmd`.
The UIs never import a specific game, so adding a game doesn't touch them.

## A game

A game implements `game.Game`:
- catalog search, prices, rules text, set sizes;
- CSV import;
- every printing of a card, and a set's full card list;
- finishes (with display names), rarities and extra filters;
- editor rows and a catalog link.

It can also implement `game.PriceHistorySource` to load daily price history (Tools → Load price history).
Game-specific card fields (Magic's mana cost, colors, Oracle ID) live in
`collection.Card.Attrs`, stored as each card's `"attrs"` object, so the collection
format is shared while each game keeps its own data.

To add a game: create `internal/games/<id>`, implement the interface, and add it to
`games()` in `cmd/tcgbinder/main.go`. It gets its own folder, `<data>/<id>/`.

## Data files

Per game, in `<data>/<game>/`:
- `collection.json`: one card per printing (keyed by its catalog ID), holding the
  copies owned as `copies`: finish → condition → list of purchases (`quantity`,
  `price_paid`, `acquired_date`), the card editor's "Your Copies". Also the game's ID and the daily value
  history.
- `price-history.json`: daily prices per printing and finish, each as a start date
  and an array of prices in cents, one per day (`-1` for a day without one). That's
  much smaller and faster than a map by date. It is only ever added to: saves are
  serialized and fsynced, a save that would lose prices is refused, and an
  unreadable file is set aside rather than overwritten.
- Caches, for example Magic's `scryfall-sets.json`.

In `<data>/`: `settings.json` (the user's settings, see `internal/settings`),
`tcgbinder.lock` (single instance), `tcgbinder.log`, and Fyne's `preferences.json`.

## Threading (Fyne)

Widgets must only be touched on Fyne's UI thread. Background work (network calls,
imports) runs in goroutines and hands results back with `runOnUI` (`fyne.Do`). The UI
is built in the app's `OnStarted` hook, because on iOS the UI loop runs on the system
main thread rather than Go's main goroutine.

## Building

- Desktop: `go build ./cmd/tcgbinder`
- iOS Simulator: `scripts/build-ios-simulator.sh`
- iOS device: `fyne package -os ios`, which needs an Apple development certificate
- Docs: `scripts/gen-docs.sh`

## Names

Code uses the UI's words, so a label on screen leads straight to the code behind it:

| UI | Code |
|---|---|
| a card in the collection | `collection.Card` (`CardKey`, `SaveCard`, `RemoveCard`) |
| Your Copies (finish › condition › purchases) | `Card.Copies`, `collection.Copies` |
| a purchase row (Add purchase) | `collection.Purchase`, `Card.AddPurchase`, `Card.PurchaseRows` |
| Cards / unique cards / sets | `Stats.Cards`, `Stats.UniqueCards`, `Stats.Sets` |
| N copies of a card | `Card.CopyCount`, `FinishCopyCount`, `ConditionCopyCount` |
| Price paid / Market value / unpriced | `PricePaid`, `MarketValue`, `Unpriced` |
| Set completion "12 / 280" | `SetStats.Collected` / `SetStats.SetSize` |
| Collector number, rules text | `Card.CollectorNumber`, `Card.RulesText` |
| Load / Update price history | `service.Game.LoadPriceHistory`, `PriceHistory.HistoryLoadedOn` |
| Card order (Settings) | `settings.Settings.CardOrder`, `collection.SortCards` |

## Card scanning

Scryfall has no image search, so a scan is identified from the text printed on the
card. Platform code reads the text, and each game identifies the card from it:

1. `scan/apple` (Objective-C, Vision) finds the card's outline
   (`VNDetectRectanglesRequest`), straightens it with `CIPerspectiveCorrection`, and
   reads its text (`VNRecognizeTextRequest`). Each line comes back with its position
   on the card, as a `scan.Reading`.
2. On iOS, the camera screen (AVFoundation) reads a few frames a second and asks Go
   about each one. `game.Scanner.ScanKey` says whether a reading is complete; the
   camera captures once two complete readings in a row agree.
3. `game.Scanner.Identify` (Magic: `games/mtg/scan.go`) picks out the title, set
   code, language, collector number and copyright year, and handles cards that are
   upside down. It looks the card up with Scryfall's `/cards/{set}/{number}`, trying
   O/0- and I/1-style corrections to the set code, and checks the result against the
   name. Failing that, it uses `/cards/named?fuzzy=` and lists every printing in
   ranked order.

`scan/apple`'s live test (`MTG_SCAN_DIR=<folder of card photos> go test
./internal/scan/apple`) reads real photos and identifies them against live Scryfall.
