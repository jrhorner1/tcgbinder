# Architecture

TCG Binder is a Rust workspace with one user interface for every platform:

```
ui/                    the user interface: plain HTML, CSS and JavaScript (no build step)
src-tauri/             the app (crate `tcgbinder`): command-line flags, the two folders, logging,
                       single-instance lock; registers the games; runs the Tauri window
                       (desktop and iPhone) or, with --web, the HTTP server (src/web.rs)
crates/api             the JSON API and the embedded UI, as one request → response handler
crates/core            game-neutral: cards and their copies, the JSON store, price history,
                       stats, filters, bulk edit; settings; JSON files shared with other
                       devices (jsonfile.rs); the Game trait; the service for
                       one game (import, prices, history, daily sync, value chart); logging,
                       the folders (datadir.rs) and changing the data folder (datafolder.rs),
                       single-instance lock
crates/mtg             Magic: Scryfall catalog, CSV and card-list importer, MTGJSON price history,
                       scan matching
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
  `price_paid`, `acquired_date`, `note`), the card editor's "Your Copies". Two
  purchases are one (`Purchase::same_as`) when price paid, date, note and lot match. Also the game's
  ID. A card's `price_from` is `"en"` when its prices are borrowed from the English
  printing (Magic's `borrow_english_prices`: Scryfall prices no other language). Kept
  in a `core::jsonfile::JsonFile` (see below).
- `price-history.json`: daily prices per printing and finish, each as a start date
  and an array of prices in cents, one per day (`-1` for a day without one). That's
  much smaller and faster than a map by date. It is only ever added to: saves are
  serialized and synced, a save that would lose prices is refused, and an unreadable
  file is set aside at startup rather than overwritten. Before each change and save,
  prices another device saved to the file are taken in (days already recorded here
  keep their price), so two devices recording prices never lose days.
- `lots.json`: lots, cards bought together for one price (`core::lots`, a
  `JsonFile`). A purchase's `lot` names its lot. `lots::lot_prices` (pure) works out
  each copy's price from the lot's total: evenly, or by market value once the lot is
  finished; `Store::reprice_lot` saves them, and `GameService::save_lot` refreshes
  stale prices before a value split. Conflict copies merge by lot ID, the later
  `created` winning.
- `sales.json`: sold cards (`core::sales`, a `JsonFile`). Each `Sale` keeps a
  `CardRef` (the card's catalog fields), the purchase it came from (cost, date, note,
  lot) and the sale. `GameService::sell` records the sale first and then takes the
  copies out (`Store::take_copies`): if the second write fails the sale and the copies
  both remain, visible and fixable, where the other order could lose the record.
  `GameService::delete_sale` can put the copies back, rebuilding a card that has left
  from its `CardRef` and refreshing its catalog data. `sales_totals` adds up proceeds,
  fees, cost and gain, leaving sales of unknown cost out of cost and gain. Conflict
  copies merge by sale ID.
- `lists.json`: lists and decks (`core::lists`, a `JsonFile`). Each `ListEntry`
  holds a catalog copy of the card (no copies), so a list shows images and prices
  without a lookup; `GameService::refresh_prices` refreshes the collection's and the
  lists' cards in one `Game::refresh`. `ListEntry::owned` counts the collection's
  copies of that printing, or with `any_printing` of every printing with the name.
  `GameService::list_entries` turns a pasted list into entries through the game's
  import; `ImportReport::approximate` marks the cards matched by name only, which
  become any-printing entries. Conflict copies merge lists by ID and, within a list,
  entries by card, the larger quantity winning.
- `backups/`: copies of `collection.json` taken before each import, bulk edit, bulk
  delete and restore (`Store::backup`), named
  `collection-YYYYMMDD-HHMMSS[-N]-<reason>.json`; the 10 newest are kept. Restoring
  accepts only a name that is listed, never a path.
- Caches, for example Magic's `scryfall-sets.json`.

Caches stay in `<local>/<game>/`. In `<local>/`: `settings.json` (this device's
settings, `core::settings`, also a `JsonFile`), `locks/<hash of the data folder>.lock`
(single instance per data folder) and `tcgbinder.log`.

### The local folder and the data folder

`core::datadir` describes the two. The **local folder** is the platform's app data
folder (Tauri's `app_data_dir`, or `datadir::default_root` for `--web`). The **data
folder** is `--data-dir` / `TCGBINDER_DATA_DIR` (shown as fixed), else the
`data_dir` setting, else the local folder. A chosen folder that doesn't exist at
startup is an error, not a new empty collection. `GameService::open(local, data)`
opens the game's caches in the first and its files in the second.

Changing the data folder (`api/src/folders.rs`, `core::datafolder`): `inspect` says
what a folder holds; `transfer` moves or copies each game's collection, price history
and backups and checks the copies read back; the choice is saved as `data_dir`, and
the app restarts (the desktop app through the `with_restart` hook; `--web` asks the
user). Until then the API refuses changes. The folder picker is another hook,
`with_folder_picker` (desktop: `tauri-plugin-dialog`).

Conflict copies (`datafolder::conflict_copies`): files named `collection*.json`
beside `collection.json`, which sync services leave when two devices save at once.
`Store::merge_copy` merges one without counting twice: the union of cards; for a
card in both, the union of purchases, the larger quantity where both have the same
purchase.

### Files another device may change

The data folder can sit in a folder a sync service shares between devices, so another
device may change these files while the app runs. `core::jsonfile::JsonFile<T>` holds
a JSON document and saves every change before returning, so memory never holds
anything the file doesn't. Each change (`mutate`) therefore needs no merging: if the
file changed on disk, it is reloaded first, then the change is applied to a copy and
saved atomically (a synced temporary file `<name>_*.tmp` renamed over the old one).
Saves are serialized, and a change that fails leaves the data as it was.

- **A change on disk** is noticed by the file's modified time and length, confirmed
  by a hash of its contents, so a file a sync service only touched isn't reloaded.
  Files modified in the last two seconds are always hashed, since some file systems
  keep times only to the second.
- **A file that doesn't parse or has gone** is most likely half-synced: nothing is
  replaced or saved over it, and the change fails with "… is being updated by another
  device; try again in a moment" (`jsonfile::FileBusy`). At startup a malformed
  collection is still an error.
- **While idle,** `GET /api/status` (the UI asks every 15 seconds) checks every game's
  files first (`GameService::refresh_files`), so the revision changes and the open page
  redraws. There is no file watcher.

Two devices saving within the same moment, before the sync service has delivered
either change, is not covered here: the service keeps both files as a conflict copy.

## Threads

Everything in `core`, `mtg` and `api` is synchronous: files and the catalog's HTTP
calls (blocking `reqwest`) block the calling thread. The app never calls them on a UI
thread: the URL-scheme handler answers each request on its own thread, and `--web`
uses `spawn_blocking`. Each game's daily sync and hourly price check run on a
background thread (`GameService::start_background_sync`). Shared state is behind
`RwLock`/`Mutex` in the store, price history and settings.

## Scryfall's rate limits

`scryfall::Client` keeps every request inside Scryfall's published limits: 2 requests
a second for `/cards/search`, `/cards/named`, `/cards/random` and `/cards/collection`,
and about 10 a second for everything else. The gap is chosen from the path. An HTTP
429 means Scryfall has limited the app for 30 seconds, so the client waits that long
(or the `Retry-After` header's time, if longer) and retries once; a second 429 fails
with "Scryfall is limiting requests". Server and network errors are retried after
short waits.

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
| Card order (Settings) | `Settings::card_order`, `SortKey::from_card_order` |
| Collection list: Sort menu | `collection::SortKey`, `collection::sort_cards` (key, direction, the game's rarity order) |
| Export CSV | `collection::write_csv`, `Game::catalog_id_label`; read back by the importer's "TCG Binder CSV" profile |
| Valuation report | `crates/api/src/report.rs` (`valuation`, and `escape` for every piece of card text) |
| Collection list: filters and More filters | `collection::Query` (core filters), `Game::filters` (Magic's Color, Color identity, Mana value) |

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
