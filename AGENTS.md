# AGENTS.md

Guidance for coding agents working in this repository. People should start with
[README.md](README.md) and [docs/contributing.md](docs/contributing.md); this file is the
short version, plus the things an agent most often gets wrong.

## Project overview

**TCG Binder** is a trading-card collection manager, starting with Magic: The
Gathering. It tracks what you paid, current market value, value over time and set
completion, imports CSV exports from other apps, and identifies cards from photos.

- **Back end:** Rust (edition 2024, MSRV 1.89), with [Tauri](https://tauri.app) 2 for
  the window.
- **UI:** Svelte 5 and TypeScript.
- **Platforms:** macOS, Windows, Linux, iPhone and iPad, and the browser
  (`tcgbinder --web`, the same UI and API served over HTTP).

Code uses the UI's words, so a label on screen leads straight to the code behind it
(e.g. "purchase" → `core::collection::Purchase`). Read `docs/contributing.md` §2
"The words" and use them in names, comments and UI text.

## Commands

```bash
cargo fmt --all                                   # format (CI checks with --check)
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked                   # all tests; offline, using fake servers
(cd ui && npm ci && npm run build)                # build the UI into ui/dist (the app serves it)
(cd ui && npm run check)                          # svelte-check; warnings fail it
```

Run them in this order before committing. CI (`.github/workflows/ci.yml`) also runs:
- the tests on Linux, macOS and Windows;
- `cargo build -p tcgbinder --locked`;
- `cargo build -p tcgbinder --lib` for `aarch64-apple-ios` and `aarch64-apple-ios-sim`.

### One test

The crates are:
- `tcgbinder-core`, `tcgbinder-mtg`, `tcgbinder-api` and `tcgbinder-vision`;
- `tcgbinder`, the app, in `src-tauri/`.

Filter tests by name or module path:

```bash
cargo test -p tcgbinder-api open_the_data_folder
cargo test -p tcgbinder-api tests::catalog -- --nocapture
cargo test -p tcgbinder-core filter::
```

Live tests are skipped unless their env var is set (README, "Tests that use live
services"), e.g. `MTG_IMPORT_FILE=$PWD/export.csv cargo test -p tcgbinder-mtg live_import -- --nocapture`.

### Run the app

Always run against a scratch folder, never a real collection. Settings, the log and
caches live in the platform's app folder under `HOME`, so point `HOME` at a scratch
folder too.

The app copies a `--data-dir` folder's `settings.json` once on first run, so putting
`{"auto_price_history":false}` there skips the ~50 MB price-history download. Build
first and run the binary directly, because `cargo` itself reads `HOME`.

```bash
mkdir -p /tmp/tcgb/data /tmp/tcgb/home && echo '{"auto_price_history":false}' > /tmp/tcgb/data/settings.json
cargo build -p tcgbinder
HOME=/tmp/tcgb/home ./target/debug/tcgbinder --web --open --data-dir /tmp/tcgb/data
```

For UI work, either:
- run `(cd ui && npm run dev)`: Vite on http://localhost:5173 with hot reload, passing
  `/api` on to 127.0.0.1:8080; or
- run `npm run build` and reload the app's own page. Debug builds read `ui/dist` from
  disk, while release and iPhone builds embed it.

Rust changes need a rebuild and restart. To get some cards in, use Tools → Import with
`crates/mtg/tests/echomtg-export.txt`, a small made-up export.

## Architecture

```
ui/                 Svelte 5 + TypeScript, built by Vite into ui/dist (rust-embed serves it)
src-tauri/          the app: flags, folders, logging, single-instance lock, registers games,
                    the Tauri window (desktop/iPhone) or the --web axum server (web.rs)
crates/api/         the JSON API: Api::handle → route, one handler file per feature, views.rs
crates/core/        game-neutral: cards, copies, the collection file, records files, price
                    history, stats, filters, settings, data folders, the Game trait, GameService
crates/mtg/         Magic: Scryfall client, CSV/list importer, MTGJSON price history, scan
                    matching, and the fake servers tests use (`testing` feature)
crates/vision/      Apple Vision (Objective-C, built with cc): text off a card photo
```

How the crates depend on each other:
- **`core`** depends on nothing else in the workspace.
- **`mtg`, `api` and `vision`** each depend only on `core`. `api` uses `mtg` only in
  its tests, through the `testing` feature.
- **`src-tauri`** joins them all, and registers each game in `games()` in
  `src-tauri/src/lib.rs`.
- **Rules that follow:** `core` never names a game, and `api` and the UI only know
  `core::game::Game`. A new game goes in its own crate.

Everything in `core`, `mtg`, `vision` and `api` is synchronous (blocking `reqwest`),
and nothing calls it on a UI thread:
- the desktop and iPhone `tcgb://` scheme handler answers each request on its own
  thread;
- `--web` uses `spawn_blocking`;
- each game's set list, daily sync and hourly price check run on named background
  threads (`GameService::start_background_sync`).

**Platform hooks.** `Api` gets OS features through `with_*` builders: opening a URL,
opening a folder in the file browser, and picking a folder. Each front end supplies
different ones:
- the desktop app fills them from Tauri plugins;
- `--web` fills only what is safe for a browser on the same computer;
- the iPhone leaves most out.

An endpoint whose hook is missing answers 501. The UI hides the control using a flag
from the API (e.g. `canOpen` and `canPick` on `GET /api/data-folder`). Follow this
pattern for anything that touches the OS.

**UI.**
- **Shared state** is in `ui/src/lib/app.svelte.ts`: `app`, `api()`, `gameAPI()` for
  per-game paths, and `status()`/`failed()`.
- **Routing** is by hash: `lib/router.svelte.ts`, `go(view, params)`.
- **Dialogs** go through `openDialog`/`askDialog` (`lib/dialogs.svelte.ts`).
- **Reloading:** pages load data through `$derived` promises that read `app.reloads`.
  The 15-second `GET /api/status` poll bumps it when the revision changes.

More: `docs/architecture.md`, `docs/web-api.md`, `docs/data-sources.md`.

## Rules (do not violate)

- **Every change is saved before the request answers.** The data folder may be synced
  between devices. `JsonFile` takes in another device's changes, applies the change,
  and writes atomically (`write_atomic`: temp file → sync → rename). Don't add
  memory-only caching of user data.
- **Requests that change data need the `X-TCGBinder-Client` header** (`CLIENT_HEADER`,
  checked in `Api::handle`; it stops other web pages changing the collection). The
  UI's `api()` adds it; the test helpers in `crates/api/src/tests/mod.rs` do too.
- **Tests never call live services.** They use the fake servers in
  `crates/mtg/src/testing.rs`. A new Scryfall query shape needs the fake taught to
  answer it.
- **Scryfall is rate-limited** (requests spaced ~110 ms, ~550 ms on search paths; a
  429 waits 30 s, or `Retry-After`, then retries once). Always use the client in
  `crates/mtg/src/scryfall/`, never a raw `reqwest`.
- **Never commit real collection data.** Fixtures are made up; CSV exports, card lists
  and `collection.json` files from a real collection stay out of the repository.
- **Don't change tests to make them pass.** A failing test means the change did
  something wrong; find out why first.
- **UI text goes in as text, never `{@html}`**, because card names, tags and lot names
  come from other apps' files. The only exception is `Icon.svelte`'s fixed SVG paths.
- **Stored file formats may change freely until the first public release.** There is
  no migration code yet. After the first release, every stored-file change must read
  the old format.

## Conventions

- **Rust:** `rustfmt.toml` sets `max_width = 140` and `use_small_heuristics = "Max"`.
  Clippy runs with `-D warnings`. Log with `tracing`, using structured fields.
- **Svelte:** components style themselves in their own `<style>`. `app.css` holds only
  shared tokens and pieces. svelte-check runs with `--fail-on-warnings`, so an unused
  CSS rule or an accessibility warning fails CI. TypeScript stays on 6.x, because
  svelte-check doesn't support 7.
- **Docs travel with the change:**
  - a new or changed endpoint gets its row in `docs/web-api.md` and a test in
    `crates/api/src/tests/<feature>.rs`;
  - a user-visible change gets its paragraph in `docs/user-guide.md`.
- **Recipes:** `docs/contributing.md` §5 has step-by-step recipes for adding a
  purchase field, filter, sort order, setting, endpoint, page, kind of record or CSV
  format.
- **Commits:** Conventional Commits with a scope, as recent history does
  (`feat(ui): …`, `fix(import): …`).

## Where things live

| What | File |
|---|---|
| `Api`, `handle`, `route` (every endpoint), platform hooks | `crates/api/src/lib.rs` |
| API handlers, one file per feature, plus `views.rs` | `crates/api/src/*.rs` |
| API tests, one file per feature, and their helpers | `crates/api/src/tests/` |
| The collection model (`Card`, `Copies`, `Purchase`, `Condition`) | `crates/core/src/collection/card.rs`, `copies.rs` |
| The collection file (open/save/backup/tags/bulk) | `crates/core/src/collection/store/` |
| Filters, sorting, queries | `crates/core/src/collection/filter.rs` |
| Stats, breakdowns, value chart, price history | `crates/core/src/collection/stats.rs`, `breakdown.rs`, `value_chart.rs`, `price_history.rs` |
| Records files (lots, lists, watchlist, sealed, sales) | `crates/core/src/records.rs`, plus one `<thing>.rs` each |
| Local folder and data folder layout | `crates/core/src/datadir.rs`, `datafolder.rs` |
| The `Game` trait / the service for one game | `crates/core/src/game.rs` / `service.rs` |
| Reading and writing files on disk | `crates/core/src/jsonfile.rs` |
| Scryfall client, importer, MTGJSON, scan, fakes | `crates/mtg/src/` |
| Desktop/iPhone app, `games()` / the web server | `src-tauri/src/lib.rs` / `web.rs` |
| UI shell, page map, startup | `ui/src/App.svelte` |
| Collection table (drawing, editing, saving) | `ui/src/collection/` |
| UI pages | `ui/src/pages/` |

## Gotchas

- **File formats:** `JsonFile` writes **pretty** JSON for the collection, records and
  settings files, and compact JSON for price history. Think about file size before
  switching either.
- **Cloning:** `Store::get` and `Store::cards` clone cards, and `cards_where` clones
  what it keeps. Don't serialize the same card repeatedly in one handler.
- **Sorting:** `Records::all()` clones and sorts, so don't call it in a loop. Use
  `read()` or `get()`.
- **Blob bodies:** the UI's `api()` sends a `Blob` body as bytes, because WebKit drops
  Blob bodies on the `tcgb://` scheme. Keep that pattern.
- **Editing a row saves the whole card** (`PUT /card`). That reprices any lot the
  card's copies were or are in, so the UI reloads lots and filters after such edits.
- **Lots:** deleting a lot keeps its cards by default, and `lot:<id>` tags travel with
  the purchase.
- **Dialogs:** never use `alert()`/`confirm()`; use `openDialog`/`askDialog`.
  `status()` can carry an Undo action.

## Performance notes

The app is sized for ordinary collections. These are known costs: design around them
rather than adding to them, and if you change a hot path, consider adding a benchmark
(none exist yet).

1. **Every mutation clones the whole document.** `JsonFile::mutate` clones the data
   (for the collection, all of `StoreData`), applies the change, serializes it and
   rewrites the file. Read the module comment before changing this: it carries the
   durability contract.
2. **`use_existing_spellings`** (`store/mod.rs`) scans every card's tags on each card
   save, to keep tag spelling canonical.
3. **No computed-data caches.** `/cards`, `/home`, `/stats`, `/filters` and `/tags`
   each clone every card and recompute everything on every request.
   `GameService::revision()` exists, so revision-keyed memoization is the intended fix.
4. **`GET /cards` sends the whole filtered list, every purchase included.** The table
   draws only visible rows; the payload is the cost.
5. **UI recomputation:** `edits.svelte.ts::rowsOf` recomputes row signatures on every
   render, and `Table.svelte` re-derives row positions and totals on each split.
6. **One bundle:** all 14 pages are statically imported in `App.svelte`.

Keep these choices as they are:
- the collection table draws only visible rows;
- `JsonFile` skips unchanged files by mtime and hash;
- Scryfall calls are batched (75 IDs) and paced;
- price history is append-only, one array per day, in cents;
- the release profile uses `lto`, `codegen-units = 1` and `strip`.
