# Web API

`tcgbinder --web` serves the browser UI and this JSON API. Every request that changes
data must include the header `X-TCGBinder-Client: <anything>` (browsers can't add it to
cross-site requests, which protects against other websites). While listening on
localhost, requests must also address the server as `localhost` or an IP address.
There is no login, so don't expose the server to untrusted networks.

`{game}` is a game ID such as `mtg`. Cards are identified by `key` (the printing's
catalog ID), passed as a query parameter. Every card in a response has `copies`
(finish → condition → purchases, the card editor's "Your Copies") and, for
convenience, `purchases`: the same purchases as a flat list of
`{"finish","condition","quantity","price_paid","acquired_date","market"}`, plus the
totals `copy_count`, `price_paid`, `market_value` and `each` (one copy in its most
valuable owned finish).

Names follow the UI: **Cards** counts every card owned and **unique cards** counts
printings; **price paid** and **market value** are the totals on the tiles.
Errors are `{"error": "..."}` with a 4xx/5xx status.

| Method & path | Purpose |
|---|---|
| `GET /api/info` | app name, version, games, chart ranges |
| `GET /api/search?q=` | search every game's collection |
| `GET /api/settings` | `{"settings", "choices", "defaults", "dataDir"}` |
| `PUT /api/settings` | change settings: send only the ones to change, e.g. `{"theme":"dark"}`; invalid values are rejected |
| `GET /api/games/{game}/home?range=90D` | Home: `cards`, `uniqueCards`, `sets`, `pricePaid`, `marketValue` (and unknown/unpriced counts), the value chart's `marketValue`/`pricePaid`/`estimate` series, `bySet`, `topCards`, `canLoadHistory`/`historyLoaded` |
| `GET /api/games/{game}/filters` | set list, rarities, finishes (with display names), game filters, conditions |
| `GET /api/games/{game}/cards?q=&set=&rarity=&finish=&f.<Filter>=` | the filtered cards (`cards`, in the Card order setting's order) with counts: `matched`, `uniqueCards`, `cardCount`, `sets` |
| `GET /api/games/{game}/card?key=` | one card: `card`, `raw` (as stored), editor `details`, catalog `link`, `others` (other printings you own) |
| `PUT /api/games/{game}/card?key=` | replace the card's purchases: `{"purchases":[{"finish","condition","quantity","price_paid","acquired_date"}, ...]}`; no copies left deletes it |
| `DELETE /api/games/{game}/card?key=` | delete a card (every copy) |
| `GET /api/games/{game}/printings?key=` | every printing of the card |
| `PUT /api/games/{game}/card/printing?key=` | move every copy to printing `{"id"}` (checked on the server) |
| `POST /api/games/{game}/bulk` | `{"keys":[...], "condition","finish","price_paid","acquired_date","delete"}` |
| `GET /api/games/{game}/set?code=` | set page: completion, value, top cards by one copy |
| `GET /api/games/{game}/missing?set=` | the set's cards you don't own |
| `GET /api/games/{game}/search?text=&set=&next=` | catalog search (one page; `next` continues) |
| `POST /api/games/{game}/cards` | add one purchase: `{"card": <search result>, "quantity", "finish", "condition", "price_paid", "acquired_date"}`; returns `newCards` and `alreadyOwned` |
| `POST /api/games/{game}/import` | import a CSV or collection JSON (multipart field `file`, or the raw body); returns `cards`, `newCards`, `alreadyOwned` |
| `GET /api/games/{game}/export` | download the collection file |
| `POST /api/games/{game}/refresh` | refresh prices now |
| `POST /api/games/{game}/price-history` | Load (or Update) price history |

Long jobs (import, refresh, price history) return `409` if the same job is already running.
