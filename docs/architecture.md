# Architecture

TCG Binder is a Rust workspace with one user interface for every platform:

```
ui/                    the user interface: plain HTML, CSS and JavaScript (no build step)
src-tauri/             the app (crate `tcgbinder`): command-line flags, data folder, logging,
                       single-instance lock; registers the games; runs the Tauri window
                       (desktop and iPhone) or, with --web, the HTTP server (src/web.rs)
crates/api             the JSON API and the embedded UI, as one request → response handler
crates/core            game-neutral: cards and their copies, the JSON store, price history,
                       stats, filters, bulk edit; settings; the Game trait; the service for
                       one game (import, prices, history, daily sync, value chart); logging,
                       data folder, single-instance lock
crates/mtg             Magic: Scryfall catalog, CSV importer, MTGJSON price history, scan matching
crates/vision          Apple's Vision framework reading text off a card photo (Objective-C)
```

**Dependency direction:** `core` ← `mtg` ← `api` ← `src-tauri`, with `vision` (built on
`core`) used only by the app. `api` works only through `core::game::Game`, so adding a
game doesn't touch it or the UI.

## One UI, three ways in

The UI talks to the app only through the JSON API (`docs/web-api.md`), with relative
URLs, so it doesn't know where it runs:

- **Desktop and iPhone (Tauri):** the window loads `tcgb://localhost/` (on Windows
  `http://tcgb.localhost/`), a URL scheme the app registers. Every request on it, the
  page's files and `/api/...` alike, goes to `Api::handle` on a worker thread. There's
  no network server and no Tauri command layer.
- **Web (`--web`):** axum hands each HTTP request to the same `Api::handle`, after
  checking the Host header (loopback only answers `localhost` and IP addresses, which
  defeats DNS rebinding).
- **Tests** call `Api::handle` directly (`crates/api/src/tests.rs`).

Requests that change data must carry the `X-TCGBinder-Client` header, which other
websites can't add to cross-site requests. Long jobs (import, price refresh, price
history) run one at a time per game. `GET /api/status` gives a revision number that
changes with every save, so the open page refreshes after background work.

The UI is embedded in release builds (`rust-embed`); debug builds read `ui/` from
disk, so reloading the page shows UI changes without rebuilding. iPhone builds always
embed it.

## A game

A game implements `core::game::Game`:
- catalog search, prices, rules text, set sizes;
- CSV import;
- every printing of a card, and a set's full card list;
- finishes (with display names), rarities and extra filters;
- editor rows and a catalog link;
- optionally a `PriceHistorySource` (Tools → Load price history) and a `Scanner`
  (card scanning).

Game-specific card fields (Magic's mana cost, colors, Oracle ID) live in `Card::attrs`,
stored as each card's `"attrs"` object, so the collection format is shared while each
game keeps its own data.

To add a game: create a crate like `crates/mtg`, implement the trait, and add it to
`games()` in `src-tauri/src/lib.rs`. It gets its own folder, `<data>/<id>/`.

## Data files

Per game, in `<data>/<game>/`:
- `collection.json`: one card per printing (keyed by its catalog ID), holding the
  copies owned as `copies`: finish → condition → list of purchases (`quantity`,
  `price_paid`, `acquired_date`), the card editor's "Your Copies". Also the game's ID
  and the daily value history. Saves are atomic (a synced temporary file renamed over
  the old one) and serialized.
- `price-history.json`: daily prices per printing and finish, each as a start date
  and an array of prices in cents, one per day (`-1` for a day without one). That's
  much smaller and faster than a map by date. It is only ever added to: saves are
  serialized and synced, a save that would lose prices is refused, and an unreadable
  file is set aside rather than overwritten.
- Caches, for example Magic's `scryfall-sets.json`.

In `<data>/`: `settings.json` (the user's settings, `core::settings`),
`tcgbinder.lock` (single instance), and `tcgbinder.log`.

## Threads

Everything in `core`, `mtg` and `api` is synchronous: files and the catalog's HTTP
calls (blocking `reqwest`) block the calling thread. The app never calls them on a UI
thread: the URL-scheme handler answers each request on its own thread, and `--web`
uses `spawn_blocking`. Each game's daily sync and hourly price check run on a
background thread (`GameService::start_background_sync`). Shared state is behind
`RwLock`/`Mutex` in the store, price history and settings.

## Names

Code uses the UI's words, so a label on screen leads straight to the code behind it:

| UI | Code |
|---|---|
| a card in the collection | `collection::Card` (`card_key`, `Store::save_card`, `Store::remove_card`) |
| Your Copies (finish › condition › purchases) | `Card::copies`, `collection::Copies` |
| a purchase row (Add purchase) | `collection::Purchase`, `Card::add_purchase`, `Card::purchase_rows` |
| Cards / unique cards / sets | `Stats::cards`, `Stats::unique_cards`, `Stats::sets` |
| N copies of a card | `Card::copy_count`, `finish_copy_count`, `condition_copy_count` |
| Price paid / Market value / unpriced | `price_paid`, `market_value`, `unpriced` |
| Set completion "12 / 280" | `SetStats::collected` / `SetStats::set_size` |
| Collector number, rules text | `Card::collector_number`, `Card::rules_text` |
| Bulk edit | `collection::BulkChange`, `Store::bulk_edit` |
| Load / Update price history | `GameService::load_price_history`, `PriceHistory::history_loaded_on` |
| Card order (Settings) | `Settings::card_order`, `collection::sort_cards` |

## Card scanning

Scryfall has no image search, so a scan is identified from the text printed on the
card. Platform code reads the text, and each game identifies the card from it:

1. `crates/vision` (Objective-C, Vision, compiled with `cc`) finds the card's outline
   (`VNDetectRectanglesRequest`), straightens it with `CIPerspectiveCorrection`, and
   reads its text (`VNRecognizeTextRequest`). Each line comes back with its position
   on the card, as a `scan::Reading`.
2. The Scan page shows the camera with the browser's camera API (`getUserMedia`, which
   the iPhone app's web view allows), posts a frame a few times a second to
   `/scan/frame`, and `Scanner::scan_key` says whether the reading is complete. It
   captures once two complete readings in a row agree, then posts the frame to
   `/scan`. A photo goes straight to `/scan`.
3. `Scanner::identify` (Magic: `crates/mtg/src/scan.rs`) picks out the title, set
   code, language, collector number and copyright year, and handles cards that are
   upside down. It looks the card up with Scryfall's `/cards/{set}/{number}`, trying
   O/0- and I/1-style corrections to the set code, and checks the result against the
   name. Failing that, it uses `/cards/named?fuzzy=` and lists every printing in
   ranked order.

The vision crate's live test reads real photos and identifies them against live
Scryfall:

```bash
MTG_SCAN_DIR=<folder of card photos> cargo test -p tcgbinder-vision live -- --nocapture
```

## Building

- Desktop: `cargo run -p tcgbinder` (or `cargo tauri build` for installers)
- iPhone: `cargo tauri ios build` (see the README)
- API docs: `cargo doc --workspace --no-deps --open`
