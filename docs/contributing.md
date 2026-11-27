# Changing the code

This guide is for someone who can program but hasn't seen this code before, or
Rust's tools, or Tauri. It says how to run the app, what the words mean, where
things are, and how to make the common kinds of change, step by step.

For how the app works as a whole, see [architecture.md](architecture.md). For the
JSON API, see [web-api.md](web-api.md).

## 1. Run it

You need Rust (install it with [rustup](https://rustup.rs)) and Node.js (a recent
version, to build the UI). On Linux, also Tauri's
[system libraries](https://tauri.app/start/prerequisites/).

Work against a **scratch folder**: an empty folder that holds a throwaway collection,
so nothing you do while testing touches your real cards. The app keeps the collection
in its data folder (`--data-dir` chooses it), and its settings, log and caches in its
own folder under your home folder; on macOS and Linux, setting `HOME` to a scratch
folder for the run keeps those apart too.

```bash
mkdir -p /tmp/tcgb/data /tmp/tcgb/home && echo '{"auto_price_history":false}' > /tmp/tcgb/data/settings.json
```

```bash
(cd ui && npm ci && npm run build)
```

```bash
cargo build -p tcgbinder
```

```bash
HOME=/tmp/tcgb/home ./target/debug/tcgbinder --web --open --data-dir /tmp/tcgb/data
```

That opens the app in your browser (`--web` serves it as a web page instead of
opening a window). The settings file stops it downloading 90 days of price history
(about 50 MB): on the first run, with no settings of its own yet, the app copies the
data folder's `settings.json`. Build first and run the built program, because
`cargo` itself reads `HOME`. To add some cards quickly, use Tools → Import with
`crates/mtg/tests/echomtg-export.txt`, a small made-up export.

Run the tests:

```bash
cargo test --workspace
```

Run the checks CI runs (formatting, the linter, the UI's types):

```bash
cargo fmt --all --check
```

```bash
cargo clippy --workspace --all-targets --locked -- -D warnings
```

```bash
(cd ui && npm run check)
```

`cargo fmt --all` (without `--check`) formats the code for you.

Working on the UI, run Vite beside the app: it reloads the page as you save a file and
passes `/api` on to the app (started as above, on port 8080 by default):

```bash
(cd ui && npm run dev)
```

and open <http://localhost:5173>. (Debug builds also read `ui/dist` from disk, so
after `npm run build` a reload of the app shows the new UI.) After changing Rust code,
stop the app, build it again and start it again.

## 2. The words

The code uses the same words as the screens. Each has a Rust type (or a TypeScript
type) that holds it.

- **Game:** a trading card game. Magic is the only one so far. A game supplies the
  catalog, prices and import (`core::game::Game`; Magic's is `mtg::Magic`). Each game
  has its own collection and files.
- **Printing:** one version of a card in the game's catalog: a card in one set, with
  its collector number and language. Scryfall's ID for it is the card's `id`.
- **Card:** one printing you own, with all your copies of it
  (`core::collection::Card`). The collection is a map from printing ID to card.
- **Copy:** one physical card. A card holds its copies as `Copies`: finish →
  condition → purchases.
- **Purchase:** copies bought together, the same in every way that matters: the same
  price paid, date and tags (`core::collection::Purchase`). It has a
  `quantity`. The card page's "Your Copies" rows are purchases.
- **Finish:** how the card is printed: `nonfoil`, `foil` or `etched` (Magic's).
- **Condition:** how worn a copy is: NM, LP, MP, HP or DMG (`core::collection::Condition`).
- **Tag:** a label you put on purchases, such as "binder A" (`Purchase::tags`;
  `core::collection::tags`). The app also writes tags of its own, starting `lot:`,
  which are never shown as tags.
- **Lot:** cards bought together for one price, which is split among them
  (`core::lots::Lot`). A purchase is in a lot when it carries the tag
  `lot:<lot ID>`. Packs and bundles are lots.
- **List:** a deck or want list: cards you want, with how many you own
  (`core::lists::List`).
- **Watch:** a card whose price you watch, with a rule for when to alert you
  (`core::watchlist::Watch`).
- **Sealed item:** an unopened box, bundle or deck, valued by hand
  (`core::sealed::SealedItem`).
- **Sale:** copies you sold, kept so earnings can be totalled
  (`core::sales::Sale`).
- **Record:** a lot, list, watch, sealed item or sale: each kind is kept in its own
  file through `core::records::Records<T>`.
- **The data folder:** where the collection lives: one folder per game holding
  `collection.json`, `price-history.json`, the records files and `backups/`. It can
  be a folder a sync service shares between devices (`core::datadir`).
- **The local folder:** this device's own folder: settings, the log, caches.
- **The service:** everything the app does for one game, with its open files
  (`core::service::GameService`).

## 3. Where things are

| Folder or file | What it is for | Open it when |
|---|---|---|
| `ui/src/app.css` | The shared styles only: colours, buttons and fields, and the few pieces many pages use (`.card-box`, `.tiles`, `.form`, `.list`, `.rows`, `.list-entry`, tag chips, dialogs). Everything else is styled in its own component's `<style>`; the collection table's rows and cells share `ui/src/collection/table.css` | You change a colour, or something every page shows |
| `ui/src/App.svelte` | Start-up, the frame (sidebar, search, tab bar), which page shows for which address | You add a page |
| `ui/src/lib/app.svelte.ts` | `app` (the shared state), `api()` and `gameAPI()` to call the server, the status line and Undo, long jobs, settings | You call the API in a new way, or need something every page can see |
| `ui/src/lib/router.svelte.ts` | `route` (the address) and `go()` to change page | Rarely |
| `ui/src/lib/dialogs.svelte.ts` | `openDialog`, `askDialog`, `confirmDialog`, `askName` | You need a new kind of dialog |
| `ui/src/lib/types.ts` | What the API sends: `Card`, `Purchase`, `Lot` and the rest | The API sends a new field the UI uses |
| `ui/src/lib/format.ts`, `tags.ts`, `cards.ts`, `lots.ts` | Money, plurals, dates and names; tag rules; adding a card; finishing a lot | You format something new |
| `ui/src/components/` | Pieces several pages use: `Modal` (every dialog's frame), `Tile`, `Bars`, `LineChart`, `TagEditor`, `ImportPreview`, `GlobalSearch` | A change shows on more than one page |
| `ui/src/pages/` | One page each: Home, Collection, Search, Lists, Missing (a set's missing cards), Tools, Import, Backups, Tags, Stats, Sealed, Watchlist, Settings, Scan | You change what one page shows or does |
| `ui/src/collection/` | The collection table: `Table.svelte`, `CardRow.svelte`, the cells, `Filters.svelte`, the lot and set panels, and `edits.svelte.ts` (the rows of copies, Split, saving and Undo) | You change the table or how it saves |
| `ui/src/card/` | A card's page (`CardPage.svelte`), its printings, watching its price, adding it to a list | You change a card's page |
| `ui/src/add/` | The Add dialog: a card, or a pack, bundle or lot and then its cards | You change adding cards |
| `crates/api/src/lib.rs` | The `Api` type, `handle` and `route` (every endpoint, grouped by file) | You add an endpoint |
| `crates/api/src/*.rs` | Handlers for each part of the API: `home`, `collection`, `catalog`, `tags`, `transfer` (import, export, backups, jobs), `settings`, `lots`, `lists`, `sales`, `watchlist`, `sealed`, `folders`, `report` (the printable valuation); `views.rs` shapes cards in answers | You change what an endpoint does |
| `crates/api/src/tests/` | API tests: `mod.rs` sets up an `Api` with a fake Scryfall; one file per feature | You add or change an endpoint |
| `crates/core/src/collection/` | The collection: `card.rs`, `copies.rs`, `tags.rs`, `filter.rs` (filters and sorting), `edit.rs` (bulk edit), `stats.rs`, `breakdown.rs` (Statistics), `price_history.rs`, `value_chart.rs`, `csv_export.rs`, and `store/` (the collection file: `mod.rs`, `backups.rs`, `tagging.rs`, `bulk.rs`) | You change what a card or purchase holds, or how the collection is filtered, totalled or saved |
| `crates/core/src/records.rs` | `Records<T>`, the store for every records file, and `DataFile` | You add a kind of record |
| `crates/core/src/lots.rs`, `lists.rs`, `watchlist.rs`, `sealed.rs`, `sales.rs` | Each kind of record and its own rules (a lot's price split, a watch's alert) | You change one feature |
| `crates/core/src/service.rs` | `GameService`: opening a game's files, refreshing prices, the daily sync, lots, selling | You change what happens across files |
| `crates/core/src/import.rs` | Importing a file, with a preview | You change import |
| `crates/core/src/settings.rs` | The settings, their choices and their file | You add a setting |
| `crates/core/src/jsonfile.rs` | Saving JSON files safely when another device changes them too | Rarely |
| `crates/core/src/datadir.rs`, `datafolder.rs` | The two folders, moving the data folder, conflict copies | You add a file to the data folder |
| `crates/core/src/game.rs` | The `Game` trait every game implements | You add a game, or something every game must supply |
| `crates/mtg/src/lib.rs` | Magic: `Magic` implements `Game` | You change something Magic-specific |
| `crates/mtg/src/scryfall/` | The Scryfall client: search, sets, card objects | You use more of Scryfall |
| `crates/mtg/src/importer/` | Reading other apps' CSV files (`formats.rs`), pasted lists (`decklist.rs`), matching rows to printings (`resolve.rs`) | You support another app's export |
| `crates/mtg/src/mtgjson.rs` | Price history and sealed products from MTGJSON | You change price history |
| `crates/mtg/src/scan.rs`, `crates/vision/` | Reading a card from a photo | You change scanning |
| `crates/mtg/src/testing.rs` | Fake Scryfall and MTGJSON servers for tests | A test needs another fake answer |
| `src-tauri/src/main.rs` | Command-line flags | You add a flag |
| `src-tauri/src/lib.rs` | The desktop and iPhone app: folders, logging, the window, the `tcgb://` scheme | You change how the app starts |
| `src-tauri/src/web.rs` | The `--web` server | You change the web server |

## 4. How a click becomes a saved change

Here is what happens when you change a group of copies' condition in the collection
table.

1. **The page.** In the collection table (`ui/src/collection/`), each copy cell is a
   small component (`ChoiceCell`, `TextCell`, `TagsCell`). Changing one calls the
   row's `edit` action, which `Table.svelte` provides: it works out which groups of
   copies change (one group on a copy's row; every group shown on a card's row; every
   ticked card's when several are ticked) and calls `saveEdits` in `edits.svelte.ts`.
2. **The request.** `saveEdits` builds each card's whole list of purchases
   (`purchasesOf`) with the change applied, and calls
   `api("PUT", gameAPI("/card?key=<printing ID>"), { purchases })`. `api()` in
   `ui/src/lib/app.svelte.ts` sends it with the `X-TCGBinder-Client` header, which
   every change needs.
3. **The route.** On the desktop and iPhone the request goes through the `tcgb://`
   scheme; with `--web` through HTTP. Either way it reaches `Api::handle` in
   `crates/api/src/lib.rs`, which calls `route`. `route` matches `("PUT", ["card"])`
   and calls `update_card`.
4. **The handler.** `Api::update_card` in `crates/api/src/collection.rs` finds the
   card, decodes each purchase into a `PurchaseForm`, checks it (`validate`) and adds
   it to the card (`add`), and calls `Store::save_card`.
5. **The store.** `Store::save_card` in `crates/core/src/collection/store/mod.rs`
   normalizes the card's tags and purchases, then calls `JsonFile::mutate`.
6. **The file.** `JsonFile::mutate` in `crates/core/src/jsonfile.rs` first takes in
   any change another device made to `collection.json`, applies this change, and
   writes the file atomically (a temporary file, renamed over the old one). The change
   is on disk before the answer goes back.
7. **The answer.** `update_card` answers `{"key", "merged", "deleted"}`. `saveEdits`
   fetches the card as saved (`GET /card`), the table shows it in place of the old
   one, and the status line says what changed, with Undo (which puts the purchases
   back as they were).

## 5. Recipes

Each recipe names the files to change, in order. When you're done, run the tests and
the checks (section 1).

### Add a field to a purchase

Follow the `acquired_date` field.

1. `crates/core/src/collection/copies.rs`: add the field to `Purchase`, with
   `#[serde(skip_serializing_if = ...)]` so an empty value stays out of the file (the
   helpers are in `crates/core/src/serde_skip.rs`). If two purchases that differ in it
   should stay apart, compare it in `Purchase::same_as`.
2. `crates/core/src/collection/store/mod.rs`: if selling must match it, compare it in
   `Store::take_copies`, and in `GameService::sell` (`crates/core/src/service.rs`).
   Add it to `Sale` (`crates/core/src/sales.rs`) and `GameService::delete_sale` if
   undoing a sale should restore it (as `acquired_date` is).
3. `crates/api/src/views.rs`: add it to `PurchaseView` and set it in `view`.
4. `crates/api/src/collection.rs`: add it to `PurchaseForm`, check it in `validate`,
   and set it in `add`.
5. If bulk edit should change it: `BulkChange` and `BulkChange::apply` in
   `crates/core/src/collection/edit.rs`, and the `Req` in `Api::bulk_edit`.
6. If the CSV export and import should carry it: `CSV_COLUMNS` and `write_csv` in
   `crates/core/src/collection/csv_export.rs`; `Row`, `Field`, `HEADER_ALIASES` and
   `parse_csv` in `crates/mtg/src/importer/formats.rs`; and the `Purchase` built in
   `crates/mtg/src/lib.rs` (`Magic::import`).
7. The UI: the field in `Purchase` (`ui/src/lib/types.ts`); in
   `ui/src/collection/edits.svelte.ts`, `Fields`, the signature that groups copies
   (`sig`) and the purchases saved (`purchasesOf`); a column in `columns.svelte.ts`
   and its cell in `CardRow.svelte` (a `ChoiceCell` or `TextCell`); and the Add
   dialog (`ui/src/add/`) and selling (`SellDialog.svelte`) if they need it.
8. Docs: the purchase fields in `docs/web-api.md`, and `docs/user-guide.md`.
9. Tests: a core test like `copies::tests` (what is stored), and an API test like
   `edit_and_delete_card` in `crates/api/src/tests/collection.rs`.

### Add a filter to the collection list

Follow the `condition` filter.

1. `crates/core/src/collection/filter.rs`: add a field to `Query` and check it in
   `Query::matcher`.
2. `crates/api/src/collection.rs`: read the parameter in `filtered_cards` (the
   `Query { ... }` it builds). A bad value is a `bad(...)` error (a 400).
3. If the filter offers choices, send them from `Api::filters` in
   `crates/api/src/home.rs` (as `conditions` is).
4. `ui/src/collection/Filters.svelte`: add the parameter's name to `MORE_KEYS`, a
   control in the More filters panel, and its label in `chipLabel`.
5. Docs: `GET /cards` in `docs/web-api.md`, and the Collection section of
   `docs/user-guide.md`.
6. Tests: a case in `more_filters` in `filter.rs`, and an API case in
   `sorting_filters_and_totals` (`crates/api/src/tests/collection.rs`).

### Add a sort order

Follow `SortKey::Rarity`.

1. `crates/core/src/collection/filter.rs`: add a variant to `SortKey`, its name in
   `SortKey::ALL`, and its comparison in `sort_cards`.
2. `ui/src/pages/Collection.svelte`: add it to `SORTS` (its menu label), and to
   `ASCENDING_FIRST` if it should start A to Z rather than largest first. If a column
   should sort by it, give the column its `sort` in `ui/src/collection/columns.svelte.ts`.
3. Docs: the `sort` list under `GET /cards` in `docs/web-api.md`.
4. Tests: the `cases` list in `every_sort_key_both_ways` in `filter.rs`, which must
   name every key.

### Add a setting

Follow `default_condition` (a choice) or `confirm_delete` (on or off).

1. `crates/core/src/settings.rs`: add the field to `Settings` and its default to
   `Settings::default`. For a choice, add its choices to `choices::ALL` and the field
   to `fields_mut` (its array grows by one), so bad values are refused and reset.
   `PUT /api/settings` (`put_settings` in `crates/api/src/settings.rs`) then saves it
   with no change there.
2. `ui/src/lib/types.ts`: the field in `Settings`. For a choice,
   `ui/src/lib/format.ts`: the label of each value in `choiceNames`.
3. `ui/src/pages/Settings.svelte`: a label and a `menu(...)` or `check(...)` in its
   section.
4. Where it takes effect: read `app.settings.<name>` in the UI (as `ask` in
   `ui/src/lib/dialogs.svelte.ts` reads `confirm_delete`), or
   `self.settings.get().<name>` in an API handler. A display setting is applied in
   `applySettings` (`ui/src/lib/app.svelte.ts`).
5. Docs: the Settings section of `docs/user-guide.md`.
6. Tests: `partial_changes` and `bad_values_and_files` in `settings.rs`, and
   `settings_api` in `crates/api/src/tests/settings.rs`.

### Add an API endpoint

Follow `GET /api/games/{game}/tags`.

1. Write the handler as a method on `Api` in the file for its feature (here
   `crates/api/src/tags.rs`): `pub(crate) fn tags(&self, g: &GameService) -> Reply`.
   It returns `Ok(Reply::Json(json!(...)))`, or an error made with `bad` (400),
   `not_found` (404), `upstream` (the catalog failed) or `internal` (500). Read a body
   with `decode`. For a new feature, add a file and a `mod` line in `lib.rs`.
2. `crates/api/src/lib.rs`: add the route to `route`, under the comment for its file.
3. A change that can be undone takes a backup first (`g.store.backup("bulk-edit")`)
   and answers `undo(g, &backup)`.
4. Docs: a row in `docs/web-api.md`.
5. Tests: in `crates/api/src/tests/<feature>.rs`, using `fixture()`, `get` and
   `call` from `tests/mod.rs`.

### Add a page to the UI

Follow the Tags page.

1. Make `ui/src/pages/<Page>.svelte`. Start it with a comment of two or three lines
   saying what the page is for. Load its data in a `$derived` promise that reads
   `app.reloads` (so it loads again after a change elsewhere), and show it with
   `{#await}`, as `Tags.svelte` does. If it needs the address's parameters, take
   `params` as a prop.
2. `ui/src/App.svelte`: import it and add it to `PAGES` under its address. If it
   belongs under Tools, add it to `UNDER` so Tools stays highlighted.
3. A way to reach it: a link in `NAV` in `App.svelte` (the sidebar), or a `ToolRow` on
   the Tools page (`ui/src/pages/Tools.svelte`).
4. Run `npm run check` in `ui/`, then look at the page at a desktop width and a phone
   width.

Pages use the shared files (`lib/`) and components (`components/`). A page may open
a dialog another part owns (the collection opens the Add dialog with `openAdd` from
`add/add.ts`), but pages don't import each other. If two pages need the same small
function, it belongs in `lib/`.

### Add a kind of record with its own file

Follow sealed product (`crates/core/src/sealed.rs`).

1. `crates/core/src/<thing>.rs`: the record type, deriving `Clone, Debug, Default,
   PartialEq, Serialize, Deserialize`, and `impl Record for <Thing>`: its `FILE`
   ("things.json"), its `NAME` for messages, `id`, `assign_id` (usually
   `self.id = new_id()`), `check` (validation), `order` (how lists are sorted) and,
   if two devices' copies need more than "keep ours", `merge`. Operations of its own
   go in an `impl Records<Thing>` block in the same file, built on `update` and
   `update_all`. Add `pub mod <thing>;` to `crates/core/src/lib.rs`.
2. `crates/core/src/service.rs`: a field `pub things: Records<Thing>` on
   `GameService`, opened in `from_store` (`Records::open(&dir)?`), and added to
   `files()` (the array grows by one).
3. `crates/core/src/datadir.rs`: add its `FILE` to `DATA_FILES`, so moving the data
   folder takes it along and its conflict copies are found.
4. `crates/api/src/folders.rs`: add it to `copy_kind` and `merge_record_copy`, so a
   conflict copy can be merged. In `ui/src/components/DataNotices.svelte`, `ABOUT`
   has the words for each kind of conflict copy.
5. The API (a new `crates/api/src/<thing>.rs`, see "Add an API endpoint") and a page
   (see "Add a page to the UI").
6. Docs: the data files in `docs/architecture.md`, and `docs/web-api.md`.
7. Tests: in the record's file (validation, ordering, merging), and the API.

### Support another app's CSV export

Follow Dragon Shield.

1. `crates/mtg/src/importer/formats.rs`: add a `Profile` to `PROFILES`, above
   `"Generic CSV"`, with a `detect` function that recognises the app's header (lower
   case, `_` and `-` read as spaces) by columns no other app has. If the app names a
   column in a way `HEADER_ALIASES` doesn't know, add the name there; if it uses a
   name that means something else elsewhere, put it in the profile's `overrides`
   instead. Set `rows_are_copies` if each row is one copy, with no quantity column.
2. Tests: a case in `formats` in `formats.rs`, with a header line copied from a real
   export and made-up rows (never real collection data).
3. Docs: the list of apps in `docs/user-guide.md` (Import), the README, the importer's
   module comment (`crates/mtg/src/importer/mod.rs`) and the Import page's text
   (`ui/src/pages/Import.svelte`).

## 6. Rules that aren't obvious

- **UI text goes in as text, never `{@html}`.** Card names, lot names and tags come
  from other apps' files and from Scryfall. Svelte puts `{name}` in as text, so they
  can't add markup or scripts to the page. (`Icon.svelte`'s fixed drawings are the
  only `{@html}`.)
- **Stored formats may change freely until the first public release.** Until then
  there are no users, so a changed file format needs no migration code: start from an
  empty data folder and import again. After the first release, every change to a
  stored file must read the old format too, or migrate it.
- **Every change is saved before the request answers.** The data folder may be
  shared with another device through a sync service, and `JsonFile` takes in the
  other device's changes before each save. If changes waited in memory, a crash would
  lose them, and two devices would overwrite each other's work.
- **Scryfall calls are paced.** Scryfall asks apps to stay under its rate limits
  (`crates/mtg/src/scryfall/mod.rs`) and to send a User-Agent naming the app. A
  client that ignores them gets blocked, for everyone using the app.
- **Tests never call live services.** They use the fake servers in
  `crates/mtg/src/testing.rs`, so they run offline, give the same answer every time,
  and don't load Scryfall. Tests that do use live services are skipped unless you
  turn them on (see the README).
- **Game-neutral code stays in `crates/core`.** Nothing there names Magic. Anything
  specific to a game goes through `core::game::Game`, so another game can be added
  without changing `core`, `api` or the UI.
- **Tests aren't changed to make them pass.** A test that fails after a change means
  the change did something the test says it shouldn't. Find out why first.
- **Never commit real collection data.** Test fixtures are made up; CSV exports,
  card lists and `collection.json` files from a real collection stay out of the
  repository.

## 7. Before you commit

Run these, in this order:

```bash
cargo fmt --all
```

```bash
cargo clippy --workspace --all-targets --locked -- -D warnings
```

```bash
cargo test --workspace --locked
```

```bash
(cd ui && npm run check && npm run build)
```

If you changed the UI, look at what you changed in the browser at a desktop width and
at a phone width (the browser's developer tools can show a phone-sized window), and
check the browser's console shows no errors.

CI (`.github/workflows/ci.yml`) runs the same checks on every pull request: the
formatting check, clippy, the UI's check and build, the tests on Linux, macOS and Windows, a
build of the app, and a check that the iPhone app compiles.
