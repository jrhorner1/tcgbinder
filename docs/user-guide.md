# User guide

TCG Binder keeps your trading card collection on your own device, looks cards up on
Scryfall, and tracks what you paid, what it's worth, and how complete your sets are.
The same app runs on the desktop (macOS, Windows, Linux), on iPhone and iPad, and in a
web browser (`tcgbinder --web`), with the same pages everywhere.

On the desktop (and in a wide browser window) the sidebar has **Home**, your
collection (**Magic**), **Tools** and **Settings**, with **Add cards** and **Scan a
card** below them. On phones the tab bar at the bottom has **Home**, **Cards**,
**Add**, **Tools** and **Settings**.

## Home

- **Tiles:** total cards (with unique cards and sets), price paid, market value, and
  gain/loss.
- **Value Over Time** (30 days, 90 days, 1 year or all):
  - *Market value* uses real daily prices. Up to 90 days come from MTGJSON, then one
    point is added for each day the app runs.
  - *Price paid* is a running total by acquired date.
  - Before recorded prices begin, a grey line estimates value at today's prices.
- **Set Completion:** distinct collector numbers owned out of each set's size. Click a
  set to open it.
- **Value by Set** and **Most Valuable Cards.** Click a set or card to open it.

Prices refresh automatically: at startup, then whenever the stored prices are from an
earlier day (checked hourly). **Settings → Update prices** can make that weekly or
manual-only, and **Tools → Refresh prices now** refreshes on demand. When prices
change in the background, the open page updates itself (unless you're typing).

Scryfall has no prices for cards printed in other languages, so a non-English card is
valued at the price of the English printing with the same set and collector number.
Such cards say **English price** in the lists and on the card page. A card with no
English printing (some promos) stays unpriced. An imported non-English card is priced
as soon as the import finishes.

## Collection

- **Browse:** every card is listed with its set, number and copies, in name order, by
  set, or most valuable first, as **Settings → Card order** says.
- **Filter:** search by name, set, collector number or a purchase's note, and filter by set, color,
  rarity and finish. The line under the filters counts unique cards, cards and sets;
  **Clear filters** shows everything again.
- **Set pages:** choose a set in the **Set** filter, or click a set on Home. They show
  completion, value, the most valuable cards (ranked by one copy), and **Missing
  Cards**. That list is loaded from Scryfall on request; click a missing card to add
  it.

On a wide screen the selected card or set shows beside the list. On a phone (or a
narrow window) a card replaces the list, with a **Back** button. When the card has
little room, its picture sits above the details and each purchase takes two lines.

### Editing a card

Each printing you own appears once. Its copies are grouped by **finish**, then
**condition**, and each condition keeps a list of **purchases**: how many copies,
the price paid for each, the date acquired, and an optional note. For example, two near-mint normal
copies bought for $0.10 and $0.12 are two purchases, so price paid and value over
time stay exact.

- **Your Copies** lists one row per purchase: finish, condition, quantity, price
  paid (each) and acquired date, with **Add a note** beneath for where the copies are
  kept, who they're for, or anything else. **Add purchase** adds a row; ✕ removes
  one. Setting every quantity to 0 removes the card when you save.
  - The finish menu lists only the finishes that printing comes in ("Normal",
    "Foil", "Etched").
  - Purchases with the same finish, condition, price, date and note are combined;
    a different note keeps them apart.
  - Cards with a note say "has a note" in the collection list, and the search box
    finds them by their note.
- **Language** is shown on every card. It belongs to the printing, so to change it,
  use **Change printing…**.
- **Change printing…** moves every copy to another printing of the same card (for
  example, a different set or language) without deleting and re-adding it. The
  **Language** menu shows the printings in that language (the card's own at
  first). Each purchase keeps its condition, price paid, date and note; a finish the
  new printing doesn't come in becomes its first finish.
- **Raw data** shows the card exactly as stored, as JSON.
- **Delete** removes every copy, after asking.

### Bulk edit

Every card in the collection list has a tick box:
1. Narrow the list with the search box and the **Set**, **Color**, **Rarity** and
   **Finish** filters.
2. Tick cards one by one, or **Select all shown** to tick everything the filters
   show. Ticks stay while you change filters, so you can build up a selection from
   several searches; the bar below the list says how many are ticked and how many
   the filters hide. **Clear** unticks everything.
3. **Bulk edit…** opens the changes for the ticked cards: condition, finish, price
   paid, acquired date or note for every copy (**Set to…** a note, or **Remove
   notes**), or delete them. You can still untick cards
   there. Finish changes skip printings that don't come in that finish, and copies
   that end up identical join one purchase.

The ticks clear once the changes are applied. The status line then offers **Undo**,
which puts the collection back as it was, as long as nothing else has changed it
since; otherwise use **Tools → Restore a backup…**. Imports offer Undo the same way.

**Tools → Bulk edit…** opens the same dialog.

## Adding cards

**Add** searches Scryfall by name, set code or both, and lists every printing, newest
first (cards you already own say so). Pick the exact one, then set the quantity,
finish, condition, price paid and date. Adding a printing you already own adds a
purchase to it (or to a matching purchase with the same finish, condition, price and
date).

- **Searching:** a name is searched as a phrase, so "sol ring" finds Sol Ring and not
  every card with "sol" and "ring" somewhere in its name. If the phrase finds
  nothing, the words are searched anywhere in a name. Scryfall's search syntax (such
  as `t:goblin cmc<=2`, or a name in quotes) is used as typed.
- **Suggestions:** after two letters, card names appear under the name box; pick one
  to search for it.
- **Adding many cards:** after adding, the page stays open with the name box empty
  and the finish, condition, price and date kept for the next card. The cards appear
  under **Added this session**, each with **Undo**, which takes back exactly those
  copies.
- **Keyboard:** Enter searches; ↑ and ↓ move through the results (or the
  suggestions); Enter then adds the selected printing.
- **Paste a list…** takes a card list, one card per line, as decklists from
  Moxfield, Archidekt or MTG Arena write them: `4 Lightning Bolt`,
  `4x Lightning Bolt`, `4 Lightning Bolt (M11) 149`, `4 Lightning Bolt [M11]`, with
  `*F*` or `(foil)` at the end for foil. Headings such as "Sideboard" and comments
  (`//`, `#`) are skipped. It shows the same preview as an import before anything is
  saved. Import accepts such a list as a file too.
- **Missing cards:** on a set page, **Add…** beside a missing card adds it without
  leaving the page.

## Scanning cards

**Scan a card** (on Add, and in the sidebar) identifies a physical card.

- **Use the camera:** hold the card inside the frame. The app reads it a few times a
  second and captures once two readings in a row agree, so there's nothing to press.
  On iPhone the camera starts as soon as you open Scan.
- **Choose a photo…** reads a photo instead (on iPhone: from your library, a new
  photo, or Files).

How it works: Apple's Vision framework finds the card, straightens it and reads its
text on your device, so scanning works in the macOS and iPhone apps (and in the web
app when it runs on a Mac). Scryfall has no image search, so the card is then looked
up by what's printed on it:
- On cards printed since 2014, the bottom-left corner shows the set code and
  collector number (e.g. `156/280 C` / `M19 • EN`). That identifies the exact
  printing, and the name confirms it.
- Older cards only show their name, so you choose the printing. Printings from
  around the card's copyright year, in the card's language, are listed first.

The result lists the printings it could be, with the add form for the first one.
When the printing is certain (the set and collector number were read), a single
button adds one copy with the finish and condition you used last. After adding, the
camera starts again for the next card. **Search by hand** helps when the card was
misread.

Tips: good light without glare on the name and the bottom-left corner, and the whole
card in view. Scanning can't tell foil from non-foil, so set the finish yourself.

## Search

The search box at the top searches your whole collection, across every game. If the
card isn't there, **Search the catalog** opens Add with the same search.

## Tools

The Tools page works on the selected game:
- **Import…:** CSV exports from EchoMTG, ManaBox, Moxfield, Deckbox, TCGplayer,
  Dragon Shield, Archidekt and others, or a collection `.json` exported from this app.
  Cards you already own gain the imported purchases. See **Importing** below.
- **Export collection:** saves the collection file, which Import accepts. Use it for
  backups and moving to another device. The desktop app saves it to your Downloads
  folder; the iPhone app saves it to its folder in the Files app; the web app
  downloads it.
- **Bulk edit…** (see above) and **Scan a card…**.
- **Restore a backup…:** a copy of the collection is saved before every import,
  bulk edit, bulk delete and restore, and the last 10 are kept. The list shows when
  each was taken, before what, and how many unique cards it holds. Restoring one
  replaces the collection (the current one is backed up first, so you can go back).
- **Refresh prices now** and **Load/Update price history**, with when each last ran.

## Importing

Choose a file on **Tools → Import…** and press **Preview**. The app reads the file and
matches every row to its exact printing on Scryfall (a minute or so for a large
file), then shows what importing would do, before anything is saved:
- how many cards and printings the file holds, how many you don't own yet and how
  many add to cards you own, and the total price paid;
- warnings about the file's columns: no quantity column (so every row counts as one
  copy), or a price or date column that mostly couldn't be read;
- rows matched by name only, whose printing may differ, so you can check them;
- rows that matched no card. **Download the unmatched rows** saves them as a CSV with
  the file's header, to fix and import again.

**Import N cards** saves them; **Cancel** saves nothing. A preview waits 15 minutes.

Files with separate regular and foil quantity columns ("Reg Qty" and "Foil Qty")
import as non-foil and foil copies. EchoMTG's export lists one copy per row; its
foil, condition, language, price acquired and date acquired columns are all
imported.

## Settings

Changes save as you make them and apply to every game. They're stored in
`settings.json` in the app's own folder (see below), so each device keeps its own,
and the desktop and web apps on one computer share them.

| Setting | Choices | Default |
|---|---|---|
| Theme | Match system, Light, Dark | Match system |
| Text size | Small, Normal, Large, Larger | Normal |
| Card images | show or hide (saves data on metered connections) | shown |
| Card order | By name, By set (newest or oldest first), Most valuable first | By name |
| Value chart | 30 days, 90 days, 1 year, All time (also changed by the chart's buttons) | 90 days |
| Update prices | Daily, Weekly, Only when I ask | Daily |
| Price history | load automatically the first time (desktop and web; phones ask) | on |
| Adding cards: condition | NM, LP, MP, HP, DMG | NM |
| Adding cards: acquired date | Today, Leave blank | Today |
| Ask before deleting cards | on or off | on |
| Log detail | Debug, Info, Warnings, Errors only (`--log-level` overrides it) | Info |

Settings also shows the data folder (see below), the version, and **Restore
defaults**.

## Where your data lives

Your collection is kept in the **data folder**. Each game has its own folder in it:
`mtg/` holds `collection.json` (your cards), `price-history.json` and `backups/`.

By default the data folder is the app's own folder:

| Platform | The app's folder |
|---|---|
| macOS | `~/Library/Application Support/io.github.jrhorner1.tcgbinder` |
| Windows | `%APPDATA%\io.github.jrhorner1.tcgbinder` |
| Linux | `~/.local/share/io.github.jrhorner1.tcgbinder` |
| iPhone, iPad | the app's private storage |

The app's folder always holds this device's `settings.json`, the log file
`tcgbinder.log` and caches.

### Choosing another folder

**Settings → Data → Change…** (desktop app) picks another data folder, for example
one in iCloud Drive, Dropbox or OneDrive, so two computers share one collection. The
web app has a box for the folder's path instead.
- An empty folder: **Move it here** takes your cards, price history and backups
  along (and deletes them from the old folder once the copies check out); **Start
  fresh** begins an empty collection there.
- A folder that already has a collection: **Use it** opens that collection. Yours
  stays where it was.

The desktop app then restarts to use the new folder; restart the web app yourself.
**Use the default folder** goes back to the app's folder. The first time you open
the app, Home offers the same choice.

If the chosen folder can't be found when the app starts (a drive that isn't
connected, or a folder not synced yet), the app says so instead of starting an
empty collection there.

When another device changes the collection, this one picks the change up within 15
seconds. If two devices save at the same moment, the sync service keeps the other
version as a separate file (such as "collection 2.json"), and Home says so:
**Merge it** adds its cards and copies to yours without counting any twice (a card
deleted on one device comes back); **Discard it** deletes that file. A backup is
taken before merging. See the README for command-line options.
