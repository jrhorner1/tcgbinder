# Data sources: Scryfall API, Scryfall bulk data, MTGJSON

How the app gets card data today, and whether Scryfall's daily bulk files or MTGJSON
should replace any of it. Figures were measured on 2026-11-07.

## What the app uses today

| Need | Source | Cost for the current collection (543 entries, 537 printings) |
|---|---|---|
| Search when adding a card | Scryfall `/cards/search` | 1 request per page of 175 results |
| Daily prices | Scryfall `/cards/collection`, 75 cards per request | 8 requests (~5 s with rate limiting) |
| Import matching | Scryfall `/cards/collection` | ~13 requests for 957 rows |
| Set sizes | Scryfall `/sets`, cached a week | 1 request |
| Missing cards / other printings | Scryfall `/cards/search` | 1–2 requests, on demand |
| Price history | MTGJSON `AllPrices.json.xz` | 48 MB once, then Scryfall daily |
| Card images | Scryfall image CDN | per image viewed, cached in memory |

## Scryfall bulk data

Scryfall publishes the whole catalog as files refreshed every day (~22:00 UTC), as
JSON and as JSON Lines (one card per line, easy to stream):

| File | Compressed download | Contents |
|---|---|---|
| Oracle Cards | 25 MB | one object per card (Oracle ID) |
| Default Cards | 81 MB | every printing, in English or its printed language |
| All Cards | 403 MB | every printing in every language |

**Cost comparison for the daily price refresh**

| Collection (unique printings) | `/cards/collection` | Default Cards bulk |
|---|---|---|
| 537 (yours) | 8 requests, ~5 s, ~2 MB | 81 MB download, ~20 s to stream-parse |
| 10,000 | 134 requests, ~70 s | same 81 MB |
| 50,000 | 667 requests, ~6 min | same 81 MB |

Scryfall's API guidance is to use bulk data rather than calling the API for large
numbers of cards. The break-even is around **5,000–10,000 unique printings**: below
that, batched API calls download far less (which matters on phones and metered
connections); above it, one bulk file is faster and kinder to Scryfall.

Bulk data would also let the app work **offline**: search, missing cards and other
printings could come from a local index instead of the API.

**Recommendation.** Keep the API for phones and small collections. Add a bulk mode:
- the web server and the desktop app switch to it automatically above ~5,000 unique
  printings (or by a setting);
- stream `default_cards` as JSON Lines and keep only the owned printings' prices
  (the same streaming approach Load price history already uses for MTGJSON, so memory stays flat);
- later, keep a slim local catalog index (name, set, number, IDs, finishes, image URL)
  to make search and missing-card lists work offline.

## MTGJSON

MTGJSON (MIT-licensed open data) is built from the same upstream sources and offers:

| File | Compressed | Use |
|---|---|---|
| `AllPrices.json.xz` | 48 MB | 90 days of daily prices from TCGplayer, Card Kingdom, Cardmarket, Mana Pool, Cardhoarder |
| `AllPricesToday.json.xz` | 4 MB | today's prices only |
| `AllPrintings.json.xz` | 100 MB | every printing with rules, legalities, identifiers |
| per-set files (e.g. `M19.json.gz`) | ~1 MB | one set, including its tokens |

**Strengths:** price *history* (Scryfall has none), several vendors including buylist
prices, rich identifiers (TCGplayer SKUs, Card Kingdom IDs) useful for import/export
with other apps, per-set downloads.

**Gaps as a Scryfall replacement:** no search API (only static files, so Add would
need a local index), no image hosting (MTGJSON points to Scryfall images), and its
files are rebuilt once a day, so brand-new cards appear later than on Scryfall.

**Recommendation.** Keep Scryfall as the catalog (search, images, card data) and
MTGJSON for price history, as today. Two cheap improvements:
- use `AllPricesToday` (4 MB) instead of the daily Scryfall refresh for very large
  collections, as an alternative to Scryfall bulk;
- offer Card Kingdom / Cardmarket prices as an optional second price, since MTGJSON
  already returns them.
