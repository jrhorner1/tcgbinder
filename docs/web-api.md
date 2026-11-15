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
`{"finish","condition","quantity","price_paid","acquired_date","note","lot","market"}`, plus the
totals `copy_count`, `price_paid`, `market_value` and `each` (one copy in its most
valuable owned finish). `price_from` is `"en"` when the card's `market` prices are
its English printing's, because the catalog has none for its language.

Names follow the UI: **Cards** counts every card owned and **unique cards** counts
printings; **price paid** and **market value** are the totals on the tiles.
Errors are `{"error": "..."}` with a 4xx/5xx status.

| Method & path | Purpose |
|---|---|
| `GET /api/info` | app name, version, games (`ID`, `Name`, and `Scan`: whether card scanning works here), chart ranges, `client` (`desktop`, `phone` or `web`), `saveExports`, `opensLinks` (links to other sites go through `POST /api/open`) |
| `GET /api/status` | `{"revision"}`: changes whenever a collection is saved or reloaded, so open pages can refresh after background work. Each call first checks every game's files for changes another device made |
| `GET /api/search?q=` | search every game's collection |
| `GET /api/data-folder` | the data folder: `path`, `isDefault`, `defaultPath`, `fixed` (set by `--data-dir` or `TCGBINDER_DATA_DIR`), `firstRun`, `canPick` (a folder picker exists here), `restartsItself`, and `conflicts`: `[{"game", "name", "kind", "cards"}]`, conflict copies (`kind`: `collection`, `lots`, `sales`, `lists`, `watchlist` or `sealed`; `cards` only for a collection) |
| `POST /api/open` | `{"url"}`: open an http or https link in the system's browser (the apps' web views drop links that open a new window); `400` for any other address, `501` where there is no opener (`--web`) |
| `POST /api/data-folder/pick` | show the system folder picker: `{"path"}`, `null` if cancelled; `501` where there is none (`--web`, iPhone) |
| `POST /api/data-folder/inspect` | `{"path"}` (absolute): what is there, `{"exists", "writable", "games": [{"id", "cards"}]}` |
| `POST /api/data-folder` | `{"path", "mode"}`: switch to `path` (`""` for the default folder). `mode` `use` opens what is there; `move` and `copy` take the current data along (refused if `path` already has a collection; `move` backs up first and deletes the originals once the copies check out). Saves the choice and answers `{"restart": true, "restartsItself", "path"}`; until the app restarts, requests that change data get `409`. `409` when the folder is fixed |
| `GET /api/settings` | `{"settings", "choices", "defaults", "dataDir"}` |
| `PUT /api/settings` | change settings: send only the ones to change, e.g. `{"theme":"dark"}`; invalid values are rejected, and so is `data_dir` (use `POST /api/data-folder`) |
| `GET /api/games/{game}/home?range=90D` | Home: `cards`, `uniqueCards`, `sets`, `pricePaid`, `marketValue` (and unknown/unpriced counts), the value chart's `marketValue`/`pricePaid`/`estimate` series, `bySet`, `topCards`, `canLoadHistory`/`historyLoaded`, `realisedThisYear` (the gain of this calendar year's sales of known cost), `hasSales`, `alerts` (the triggered watches, as `GET /watchlist` gives them), and `sealed` (`{"value", "items", "unvalued"}`, null without sealed product; not part of `marketValue`) |
| `GET /api/games/{game}/stats` | the Statistics page: `breakdowns` (`[{"id", "name", "param", "rows": [{"key", "cards", "unique", "value", "paid"}]}]`: rarity, finish, condition, language, release year, then the game's own, Magic's Color and Card type; `param` is the `GET /cards` filter a row's `key` goes in, null for release year), `bands` (`[{"label", "min", "max", "cards", "unique", "value", "share"}]`, by one copy's price, plus "No price" when any card has none), `byMonth` (`[{"month", "cards", "paid"}]`), `undated` (copies without an acquired date), `mostCopies` (up to 10 cards with more than one copy) and `tags` (as `GET /tags` gives them) |
| `GET /api/games/{game}/tags` | every tag in use with its totals over the purchases that carry it: `{"tags": [{"name", "cards", "copies", "value", "paid"}], "untagged": {...} or null}` (`cards` counts printings with a tagged purchase; `untagged` totals the copies with no tags) |
| `PUT /api/games/{game}/tags?name=` | rename a tag everywhere: `{"name": "<new>"}`. If the new name is already a tag (ignoring case) the two become one, with its existing spelling; a change of case alone respells it everywhere. Purchases made identical merge. Returns `changed` (purchases) and `undo`; 404 if no card has the tag, 400 for an invalid name |
| `DELETE /api/games/{game}/tags?name=` | remove a tag from every purchase; returns `changed` and `undo`; 404 if no card has it |
| `GET /api/games/{game}/filters` | set list, rarities, finishes (with display names), game filters, conditions, `languages` (`[{"code", "name"}]`) and `tags` (the tag names in use, sorted ignoring case) |
| `GET /api/games/{game}/cards?q=&set=&rarity=&finish=&f.<Filter>=&…&sort=&dir=` | the filtered, sorted cards (`cards`) with counts (`matched`, `uniqueCards`, `cardCount`, `sets`), totals for them (`marketValue`, `pricePaid`), and the order used (`sort`, `dir`). Filters: `q` (name, set, number, or a purchase's note or tags), `set`, `rarity`, `finish`, `condition` (a code such as `NM`; any copy in it), `language`, `card_type` (the type line contains it), `lot` (a lot's ID), `price_min` / `price_max` (one copy's price; unpriced cards match neither), `acquired_from` / `acquired_to` (`YYYY-MM-DD`; any purchase in range), `min_copies`, the flags `has_note`, `no_price_paid`, `unpriced`, `untagged` (a purchase with no tags) (`1` or `true`), `tags` (comma-separated; with `tags_mode=all`, the default, the card must have all of them across its purchases; with `tags_mode=any`, at least one), `tags_not` (comma-separated: tags the card must not have), and one per game filter, `f.<Filter name>` (Magic: `f.Color`, `f.Color identity`, `f.Mana value` with `0`–`6` or `7+`). `sort` is one of `name`, `set`, `number`, `price`, `value`, `gain`, `paid`, `quantity`, `acquired`, `released`, `rarity`, `tag` (a card's first tag alphabetically; untagged cards last either way); `dir` is `asc` (the default) or `desc`. Without `sort`, the Card order setting (`name` asc, `set` desc, `set` asc or `value` desc). Tag names compare ignoring case; a tag no card has matches nothing. A bad number, date, sort, direction or `tags_mode` is a 400. Each purchase in `cards` carries `tags`. |
| `GET /api/games/{game}/card?key=` | one card: `card`, `raw` (as stored), editor `details`, catalog `link`, `others` (other printings you own) |
| `PUT /api/games/{game}/card?key=` | replace the card's purchases: `{"purchases":[{"finish","condition","quantity","price_paid","acquired_date","note","lot","tags"}, ...]}`; no copies left deletes it. `tags` is an array of strings (trimmed, spaces collapsed, duplicates ignoring case dropped, at most 20 of at most 40 characters, no comma or semicolon; an invalid tag is a 400 with the reason); a tag matching one in the collection apart from case takes the collection's spelling. A purchase sent without `tags` has none |
| `DELETE /api/games/{game}/card?key=` | delete a card (every copy) |
| `GET /api/games/{game}/printings?key=&lang=` | every printing of the card in a language (default: the card's own) |
| `PUT /api/games/{game}/card/printing?key=` | move every copy to printing `{"id", "lang"}` (checked on the server among that language's printings) |
| `POST /api/games/{game}/bulk` | `{"keys":[...], "condition","finish","price_paid","acquired_date","note","add_tags","remove_tags","clear_tags","delete"}` (`note`: leave it out to keep notes, `""` to remove them; tags are cleared, then removed, then added, on every purchase). A card that would end up with more than 20 tags on a purchase is a 400 naming it, and nothing changes. Returns `Updated`, `Deleted`, `Skipped` and `undo` |
| `GET /api/games/{game}/lots` | every lot, newest first: the lot (`id`, `name`, `acquired_date`, `total_paid`, `count`, `split`, `note`, `open`, `created`, `split_date`) with `entered`, `copies`, `cards`, `expected` (null when counted as entered), `priceEach` (the even price, while open or for an even split) and `marketValue` |
| `POST /api/games/{game}/lots` | create a lot: `{"name", "total_paid", "acquired_date", "count", "split", "note"}`. `count` is `{"kind": "known", "cards"}`, `{"kind": "bundle", "packs", "cards_per_pack", "extra_cards", "lands", "lands_share_cost"}` or `{"kind": "as_entered"}`; `split` is `by_value` or `even` |
| `GET /api/games/{game}/lots?id=` | one lot and its `cards` |
| `PUT /api/games/{game}/lots?id=` | change a lot's fields, `"open": false` to finish it, `"open": true` to continue it, or `"recalculate": true` to reprice at today's prices; changing the total, count, date or split reprices its cards. Returns `lot`, `note`, `short` (`{"entered", "expected"}` when a lot is finished short) and `outcome` (a value split's `paid`, `marketValue` and `percent`) |
| `POST /api/games/{game}/lots/cards?id=` | put cards already in the collection into a lot (after a backup): `{"cards": [{"key", "quantity"}]}`, `quantity` optional (every copy not in the lot; copies in no lot are taken first). The copies take the lot's date; the lot, and any lot they left, are repriced. Returns `lot`, `moved` and `undo`; 400 if a card has fewer copies to move |
| `DELETE /api/games/{game}/lots/cards?id=&key=` | take a card out of a lot: its copies stay in the collection at the price the lot gave them, and the lot is repriced. Returns `lot` and `freed`; 400 if the card isn't in the lot |
| `DELETE /api/games/{game}/lots?id=&cards=keep\|remove` | delete a lot: `keep` leaves its copies with their prices, `remove` deletes them (after a backup; returns `undo`) |
| `POST /api/games/{game}/card/sell?key=` | sell copies from one purchase: `{"finish", "condition", "price_paid", "acquired_date", "note", "tags"}` name the purchase, then `{"quantity", "sale_each", "date", "fees", "sale_note"}` (`date` defaults to today, `fees` is for the whole sale). The sale is recorded, then the copies leave the collection. Returns `sale` and `gain` (null when the purchase has no price paid). 400 when the purchase isn't there or holds fewer copies |
| `GET /api/games/{game}/sales?year=` | sales, newest first (all years, or one): each sale (`id`, `date`, `card` (`id`, `name`, `set`, `set_name`, `collector_number`, `language`, `image_uri`, `rarity`, `released_at`), `finish`, `condition`, `quantity`, `cost_each`, `acquired_date`, `sale_each`, `fees`, `note`) with `proceeds` and `gain`; `totals` and `months` (`[{"month", ...}]`), each `{"sales", "copies", "proceeds", "fees", "cost", "gain", "unknownCostCopies"}` (sales without a price paid are left out of `cost` and `gain`); `years` with sales |
| `DELETE /api/games/{game}/sales?id=&restore=1` | delete a sale; with `restore=1` its copies go back into the collection as the purchase they were |
| `GET /api/games/{game}/lists` | every list, by name, with totals: `id`, `name`, `notes`, `created`, `entries`, `cards` (copies wanted), `owned` (of those), `missing`, `marketValue`, `missingCost` (the missing copies at market price) |
| `POST /api/games/{game}/lists` | create a list: `{"name", "notes"}`; returns its totals |
| `GET /api/games/{game}/lists?id=` | one list: `list` (totals) and `entries`: `[{"index", "card", "quantity", "any_printing", "finish", "owned", "price"}]` (`owned` counts that printing, or with `any_printing` every printing with the name, in `finish` if set) |
| `PUT /api/games/{game}/lists?id=` | change any of `name`, `notes` and `entries` (`[{"card", "quantity", "any_printing", "finish"}]`, replacing them all) |
| `DELETE /api/games/{game}/lists?id=` | delete a list |
| `POST /api/games/{game}/lists/entries?id=` | add a catalog card: `{"card", "quantity", "any_printing", "finish"}`; the same card, finish and any-printing choice grows the existing entry |
| `GET /api/games/{game}/lists/export?id=` | download the list as decklist text: `4 Lightning Bolt (M11) 149`, `*F*`/`*E*` for foil or etched, just the name for any printing |
| `POST /api/games/{game}/lists/import?id=` | add a pasted card list (the raw body; any import file works): lines matched by name only count any printing. Returns `list`, `added`, `notes` and `unmatched` (the lines not found) |
| `GET /api/games/{game}/sealed` | sealed product: `items` (`id`, `product_uuid`, `name`, `set`, `category`, `subtype`, `quantity`, `price_paid`, `acquired_date`, `value`, `value_date`, `note`, `tcgplayer_product_id`, `purchase_url`, with `valueStale` (no value, or one over 90 days old) and `gain` (null unless both prices are set)) and `totals` (`items`, `paid`, `value`, `unvalued`, `stale`) |
| `POST /api/games/{game}/sealed` | add an item with those fields (`name` and `quantity` required); a value without `value_date` is dated today |
| `PUT /api/games/{game}/sealed?id=` | replace an item's fields; a changed value keeping its old `value_date` is dated today |
| `DELETE /api/games/{game}/sealed?id=` | remove an item |
| `GET /api/games/{game}/sealed/products?set=` | the catalog's sealed products for a set (Magic: MTGJSON's `sealedProduct`): `products` (`uuid`, `name`, `set`, `category`, `subtype`, `release_date`, `tcgplayer_product_id`, `purchase_url`) |
| `GET /api/games/{game}/watchlist` | the watched cards, triggered first: `key` (`<catalog ID>/<finish>`), `card`, `finish`, `added`, `baseline`, `baseline_date`, `rule` (`{"kind": "percent"\|"above"\|"below", "value"}`), `triggered` (`{"date", "price"}` or absent), `price`, `change` and `changePercent` (from the baseline; null without one) |
| `POST /api/games/{game}/watchlist` | watch a catalog card: `{"card", "finish", "rule"}` (default a 20% move). The baseline is its current price in that finish; a percent rule needs one. 400 if already watched |
| `PUT /api/games/{game}/watchlist?key=` | `{"rule"}` changes the alert; `{"action": "dismiss"}` clears a triggered alert (keeping the baseline); `{"action": "reset"}` makes today's price the baseline |
| `DELETE /api/games/{game}/watchlist?key=` | stop watching |
| `GET /api/games/{game}/backups` | the collection's backups, newest first: `[{"name", "date", "reason", "cards", "size"}]` (`reason`: `import`, `bulk-edit`, `bulk-delete`, `restore`, `merge` or `move`; `cards` counts unique cards) |
| `POST /api/games/{game}/conflicts/merge` | `{"name"}`: merge a listed conflict copy into the collection (after a backup; returns `undo`), the lots, the sales, the lists (by ID; a list's entries by card, the larger quantity winning) the watchlist (by card and finish, the later `baseline_date` winning) or the sealed product (by ID), then delete it; returns `changed` |
| `POST /api/games/{game}/conflicts/discard` | `{"name"}`: delete a listed conflict copy |
| `POST /api/games/{game}/backups/restore` | `{"name", "ifRevision"}`: replace the collection with a listed backup, after backing up the current one; returns `cards` (unique cards) and `undo`. With `ifRevision`, `409` unless the collection is still at that revision. `404` for a name not listed |
| `GET /api/games/{game}/set?code=` | set page: completion (`collected` of `size`, following the `set_completion` setting), value, top cards by one copy |
| `GET /api/games/{game}/missing?set=` | the set's cards you don't own (`missing`), following the `set_completion` setting: with `main` and a known printed size, only plain collector numbers up to it. Also `total` (the set's cards counted), `printedSize` (null when every card counts), `totalCost` (one of each missing card at market price, in the usual finish that has one) and `unpriced` |
| `GET /api/games/{game}/search?text=&set=&next=` | catalog search, newest printings first (one page; `next` continues); plain text is searched as a phrase in names, falling back to its words anywhere in a name; `owned` gives the copies you own of each result |
| `GET /api/games/{game}/autocomplete?q=` | `{"names"}`: card names starting like `q` (from 2 characters) |
| `POST /api/games/{game}/cards` | add one purchase: `{"card": <search result>, "quantity", "finish", "condition", "price_paid", "acquired_date", "note", "lot", "tags"}`; returns `newCards` and `alreadyOwned`. With `lot` (an open lot's ID), its price and date replace those sent, the lot is repriced, and the answer has its summary as `lot` |
| `POST /api/games/{game}/import` | import a CSV, a pasted card list (one card per line, e.g. `4 Lightning Bolt (M11) 149`) or collection JSON (multipart field `file`, or the raw body); returns `summary`, `format`, `cards`, `newCards`, `alreadyOwned` |
| `POST /api/games/{game}/import?preview=1` | read and match the file without saving anything: `token`, `format`, `rows` (read from the file), `cards` (printings), `copies`, `newCards`, `alreadyOwned`, `pricePaid`, `notes` (column warnings first, then rows to check), `unmatched` (`[{"line", "text"}]`: rows that matched no card, as written in the file) and `header` (the file's header line). One preview is kept per game for 15 minutes; a new one replaces it |
| `POST /api/games/{game}/import/commit` | save a preview: `{"token", "lot", "tags"}` (`lot` optional: the cards go into that open lot; `tags` optional: added to every imported purchase, checked before the preview is used); returns what `import` returns. `410` if the preview was already saved, replaced, or is too old |
| `GET /api/games/{game}/export?format=` | download an export. `format=json` (the default): the collection file. `format=csv`: one row per purchase (`Name, Set Code, Set Name, Collector Number, Language, Finish, Condition, Quantity, Price Paid, Acquired Date, Note, Tags, Rarity, Market Price` and the game's catalog ID; `Tags` joined with "; ", `Scryfall ID` for Magic), which import reads back. `format=report`: a self-contained, printable HTML valuation. CSV and report take the same filter and sort parameters as `GET /cards`; any other `format` is a 400 |
| `POST /api/games/{game}/export?format=` | save the same export instead (apps that can't download files: the desktop app's Downloads folder, the iPhone app's Files folder); returns `path` |
| `POST /api/games/{game}/scan` | identify the card in a photo (the raw image as the body): `read` (what was read), `name`, `exact`, `note`, `candidates` (printings, best first), `owned`, `text`; `501` where scanning isn't available (it needs macOS or iOS) |
| `POST /api/games/{game}/scan/frame` | read one camera frame: `key` (what it says about the card), `complete` (enough to look it up), `lines` |
| `POST /api/games/{game}/refresh` | refresh prices now |
| `POST /api/games/{game}/price-history` | Load (or Update) price history |

Imports, bulk changes and restores back up the collection first and answer `undo`:
`{"backup", "revision"}`. Sending `{"name": backup, "ifRevision": revision}` to
`backups/restore` undoes the change, as long as nothing else has changed the
collection since.

Long jobs (import and its preview, refresh, price history) return `409` if the same job
is already running. `GET /api/games/{game}/jobs` gives the game's running jobs and how far
each has got: `{"jobs": [{"name", "done", "total", "message"}]}` (`done` and `total` are
0 when a job can't count its steps).
Request bodies (an import file, a photo) can be up to 64 MB.
