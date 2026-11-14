# Web API

Every TCG Binder front end uses this JSON API: the desktop and iPhone apps call it
in-process through their `tcgb://` URL scheme, and `tcgbinder --web` serves it over
HTTP along with the UI. Every request that changes data must include the header
`X-TCGBinder-Client: <anything>` (browsers can't add it to cross-site requests, which
protects against other websites). While listening on localhost, `--web` also requires
requests to address it as `localhost` or an IP address. There is no login, so don't
expose the server to untrusted networks.

`{game}` is a game ID such as `mtg`. Cards are identified by `key` (the printing's
catalog ID), passed as a query parameter. Every card in a response has `copies`
(finish → condition → purchases, the card editor's "Your Copies") and, for
convenience, `purchases`: the same purchases as a flat list of
`{"finish","condition","quantity","price_paid","acquired_date","note","market"}`, plus the
totals `copy_count`, `price_paid`, `market_value` and `each` (one copy in its most
valuable owned finish). `price_from` is `"en"` when the card's `market` prices are
its English printing's, because the catalog has none for its language.

Names follow the UI: **Cards** counts every card owned and **unique cards** counts
printings; **price paid** and **market value** are the totals on the tiles.
Errors are `{"error": "..."}` with a 4xx/5xx status.

| Method & path | Purpose |
|---|---|
| `GET /api/info` | app name, version, games (`ID`, `Name`, and `Scan`: whether card scanning works here), chart ranges, `client` (`desktop`, `phone` or `web`), `saveExports` |
| `GET /api/status` | `{"revision"}`: changes whenever a collection is saved or reloaded, so open pages can refresh after background work. Each call first checks every game's files for changes another device made |
| `GET /api/search?q=` | search every game's collection |
| `GET /api/data-folder` | the data folder: `path`, `isDefault`, `defaultPath`, `fixed` (set by `--data-dir` or `TCGBINDER_DATA_DIR`), `firstRun`, `canPick` (a folder picker exists here), `restartsItself`, and `conflicts`: `[{"game", "name", "cards"}]`, conflict copies of a collection |
| `POST /api/data-folder/pick` | show the system folder picker: `{"path"}`, `null` if cancelled; `501` where there is none (`--web`, iPhone) |
| `POST /api/data-folder/inspect` | `{"path"}` (absolute): what is there, `{"exists", "writable", "games": [{"id", "cards"}]}` |
| `POST /api/data-folder` | `{"path", "mode"}`: switch to `path` (`""` for the default folder). `mode` `use` opens what is there; `move` and `copy` take the current data along (refused if `path` already has a collection; `move` backs up first and deletes the originals once the copies check out). Saves the choice and answers `{"restart": true, "restartsItself", "path"}`; until the app restarts, requests that change data get `409`. `409` when the folder is fixed |
| `GET /api/settings` | `{"settings", "choices", "defaults", "dataDir"}` |
| `PUT /api/settings` | change settings: send only the ones to change, e.g. `{"theme":"dark"}`; invalid values are rejected, and so is `data_dir` (use `POST /api/data-folder`) |
| `GET /api/games/{game}/home?range=90D` | Home: `cards`, `uniqueCards`, `sets`, `pricePaid`, `marketValue` (and unknown/unpriced counts), the value chart's `marketValue`/`pricePaid`/`estimate` series, `bySet`, `topCards`, `canLoadHistory`/`historyLoaded` |
| `GET /api/games/{game}/filters` | set list, rarities, finishes (with display names), game filters, conditions, and `languages` (`[{"code", "name"}]`) |
| `GET /api/games/{game}/cards?q=&set=&rarity=&finish=&f.<Filter>=` | the filtered cards (`q` matches name, set, number or a purchase's note) (`cards`, in the Card order setting's order) with counts: `matched`, `uniqueCards`, `cardCount`, `sets` |
| `GET /api/games/{game}/card?key=` | one card: `card`, `raw` (as stored), editor `details`, catalog `link`, `others` (other printings you own) |
| `PUT /api/games/{game}/card?key=` | replace the card's purchases: `{"purchases":[{"finish","condition","quantity","price_paid","acquired_date","note"}, ...]}`; no copies left deletes it |
| `DELETE /api/games/{game}/card?key=` | delete a card (every copy) |
| `GET /api/games/{game}/printings?key=&lang=` | every printing of the card in a language (default: the card's own) |
| `PUT /api/games/{game}/card/printing?key=` | move every copy to printing `{"id", "lang"}` (checked on the server among that language's printings) |
| `POST /api/games/{game}/bulk` | `{"keys":[...], "condition","finish","price_paid","acquired_date","note","delete"}` (`note`: leave it out to keep notes, `""` to remove them); returns `Updated`, `Deleted`, `Skipped` and `undo` |
| `GET /api/games/{game}/backups` | the collection's backups, newest first: `[{"name", "date", "reason", "cards", "size"}]` (`reason`: `import`, `bulk-edit`, `bulk-delete`, `restore`, `merge` or `move`; `cards` counts unique cards) |
| `POST /api/games/{game}/conflicts/merge` | `{"name"}`: merge a listed conflict copy into the collection (after a backup), then delete it; returns `changed` and `undo` |
| `POST /api/games/{game}/conflicts/discard` | `{"name"}`: delete a listed conflict copy |
| `POST /api/games/{game}/backups/restore` | `{"name", "ifRevision"}`: replace the collection with a listed backup, after backing up the current one; returns `cards` (unique cards) and `undo`. With `ifRevision`, `409` unless the collection is still at that revision. `404` for a name not listed |
| `GET /api/games/{game}/set?code=` | set page: completion, value, top cards by one copy |
| `GET /api/games/{game}/missing?set=` | the set's cards you don't own |
| `GET /api/games/{game}/search?text=&set=&next=` | catalog search (one page; `next` continues); `owned` gives the copies you own of each result |
| `POST /api/games/{game}/cards` | add one purchase: `{"card": <search result>, "quantity", "finish", "condition", "price_paid", "acquired_date", "note"}`; returns `newCards` and `alreadyOwned` |
| `POST /api/games/{game}/import` | import a CSV or collection JSON (multipart field `file`, or the raw body); returns `summary`, `format`, `cards`, `newCards`, `alreadyOwned` |
| `POST /api/games/{game}/import?preview=1` | read and match the file without saving anything: `token`, `format`, `rows` (read from the file), `cards` (printings), `copies`, `newCards`, `alreadyOwned`, `pricePaid`, `notes` (column warnings first, then rows to check), `unmatched` (`[{"line", "text"}]`: rows that matched no card, as written in the file) and `header` (the file's header line). One preview is kept per game for 15 minutes; a new one replaces it |
| `POST /api/games/{game}/import/commit` | save a preview: `{"token"}`; returns what `import` returns. `410` if the preview was already saved, replaced, or is too old |
| `GET /api/games/{game}/export` | download the collection file |
| `POST /api/games/{game}/export` | save a copy of the collection file instead (apps that can't download files: the desktop app's Downloads folder, the iPhone app's Files folder); returns `path` |
| `POST /api/games/{game}/scan` | identify the card in a photo (the raw image as the body): `read` (what was read), `name`, `exact`, `note`, `candidates` (printings, best first), `owned`, `text`; `501` where scanning isn't available (it needs macOS or iOS) |
| `POST /api/games/{game}/scan/frame` | read one camera frame: `key` (what it says about the card), `complete` (enough to look it up), `lines` |
| `POST /api/games/{game}/refresh` | refresh prices now |
| `POST /api/games/{game}/price-history` | Load (or Update) price history |

Imports, bulk changes and restores back up the collection first and answer `undo`:
`{"backup", "revision"}`. Sending `{"name": backup, "ifRevision": revision}` to
`backups/restore` undoes the change, as long as nothing else has changed the
collection since.

Long jobs (import, refresh, price history) return `409` if the same job is already running.
Request bodies (an import file, a photo) can be up to 64 MB.
