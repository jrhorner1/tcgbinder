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
  gain/loss. Gain/loss is for the cards you hold; once you've sold cards, its detail
  line adds the gain realised by this year's sales.
- **Value Over Time** (30 days, 90 days, 1 year or all):
  - *Market value* uses real daily prices. Up to 90 days come from MTGJSON, then one
    point is added for each day the app runs.
  - *Price paid* is a running total by acquired date.
  - Before recorded prices begin, a grey line estimates value at today's prices.
- **Set Completion:** distinct collector numbers owned out of each set's size. Click a
  set to open it.
- **Value by Set** and **Most Valuable Cards.** Click a set or card to open it.

Long jobs (importing, refreshing prices, loading price history) show their progress in
the status line at the bottom. Confirmations, such as before deleting, appear in the
app's own dialog. Every row in a list can be reached with Tab and opened with Enter.

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

- **Browse:** every card is listed with its set, number and copies, and on the right
  what all its copies are worth (with the price of one copy underneath when there are
  several in one finish). With card images on, each row has a small picture.
- **Sort:** the **Sort** menu above the list orders by name, set (release date, then
  collector number), number, price of one copy, value of every copy, gain or loss
  (cards with no price paid last), price paid, quantity, acquired date (the earliest
  purchase; undated cards last), release date, or rarity. The arrow beside it turns
  the order around. Choosing a sort starts with the most first, except for name and
  number. Until you choose one, the list follows **Settings → Card order**.
- **List or grid:** **Grid** shows the cards as pictures, with the number of copies
  and their value on each.
- **Filter:** search by name, set, collector number or a purchase's note, and filter by
  set, color, rarity and finish. **More filters** adds color identity, mana value,
  condition (any copy in it), language, lot, type ("creature" matches every type line
  with it), price of one copy (from and to), acquired date (any purchase in the
  range), a minimum number of copies, and cards that have a note, have a copy with no
  price paid, or have a copy with no market price. Each filter in use shows as a chip
  under the filters; click the chip to remove it.
- The line under the filters counts unique cards, cards and sets, and adds up the
  market value and price paid of the cards shown. **Clear filters** shows everything
  again and keeps the sort and view. Sort, view and filters are part of the page's
  address, so a bookmark keeps them.
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

## Adding a lot

When you buy many cards for one price (a bulk lot, a bundle), **Add a lot…** on the
Add page records the purchase once and works out each card's price for you:
- **Name, total paid and date.** Every card in the lot gets the date.
- **How many cards?**
  - **I counted them:** one number.
  - **It's a bundle:** packs × cards per pack (14 by default), other cards, and
    basic lands. Basic lands cost nothing unless **Lands share the cost** is ticked.
  - **I'll count as I add:** the total is divided by the cards entered so far.
- **Split the cost:**
  - **By each card's market value** (the default): a card worth more costs more. If
    the lot's cards are worth $80 and it cost $55, each card is costed at 68.75% of
    its market price. A card without a price is valued at the average of the lot's
    cards of its rarity.
  - **Evenly:** every card costs the same, kept to four decimal places so the lot
    adds up to its total ($38 for 500 cards is $0.0760 each).

While a lot is open, the Add page shows it with how many cards are entered, **Pause**
and **Finish**. Cards you add (by search, scan or **Paste a list…**) belong to the
lot, which sets their price and date. Until it's finished they carry an even share.
**Finish** works out a value split from that day's prices (refreshing older ones
first), showing what each card will be costed at before saving. If you entered fewer
cards than counted, it asks whether to spread the total over the cards entered.

A paused lot is offered again the next time you open Add, on any device sharing the
data folder. **Tools → Lots…** lists every lot with what it cost and what its cards
are worth now. A lot's page lists its cards, with **Edit** (changing the total, count,
date or split reprices the cards), **Continue adding**, **Recalculate at today's
prices** (finished value-split lots) and **Delete** (keep the cards with their prices,
or delete them too). On a card's page, copies from a lot name it; a price typed there
is replaced the next time the lot is repriced.

## Selling cards

To record a sale, open the card and choose **Sell…** under the purchase the copies
came from. Say how many copies, what each sold for, the date, any fees for the whole
sale, and a note (where or to whom). The dialog shows the gain as you type: what they
sold for, less fees and what you paid. The copies leave your collection (a card with
none left leaves the list), and the sale is kept even after the card is gone.

**Tools → Earnings…** lists your sales, newest first, for all years or one year: what
they sold for, fees, what the copies cost, and the gain, in total and by month. Copies
with no price paid count toward what you sold for, but their gain is unknown, so they
are left out of the cost and gain totals (the page says how many). **Undo…** on a sale
either puts the copies back in your collection as the purchase they were, or deletes
only the record and leaves them sold.

Deleting a card is still the way to correct a mistake; it records no sale.

## Lists and decks

**Lists** (in the sidebar; on a phone, **Tools → Lists…**) keeps decks and want
lists. Each list shows how many of its cards you own, what the missing ones would
cost at market price, and what the whole list is worth.

- **New list** makes one. **Add to list…** on the Add page's card form, or **To
  list…** on a set's Missing Cards, puts a card on a list (or a new one).
- **Paste a list…** on a list's page adds a decklist as Moxfield, Archidekt or MTG
  Arena export it. A line with a set (and number) wants that printing; a line with
  just a name counts any printing as owned.
- Each entry says how many you own of how many you want. With **Any printing**
  ticked, every printing with the card's name counts; otherwise only that printing
  (and, for a foil entry, only foil copies). Change the number wanted in place, or
  remove an entry with ✕. **Add…** on a missing entry opens the add form.
- **Rename**, **Notes**, **Export** (the list as decklist text, which Paste a list
  reads back) and **Delete** (the cards in your collection stay).

The cards on lists get their prices refreshed along with your collection.

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
card isn't there, **Search the catalog** opens Add with the same search. It shows the first 200 matches per game; **Show all N** opens the collection list
with the same search.

## Tools

The Tools page works on the selected game:
- **Import…:** CSV exports from EchoMTG, ManaBox, Moxfield, Deckbox, TCGplayer,
  Dragon Shield, Archidekt and others, this app's own CSV, or a collection `.json`
  exported from this app.
  Cards you already own gain the imported purchases. See **Importing** below.
- **Export collection:** saves the collection file, which Import accepts. Use it for
  backups and moving to another device. The desktop app saves it to your Downloads
  folder; the iPhone app saves it to its folder in the Files app; the web app
  downloads it. The other exports go to the same place.
- **Export CSV:** every card as a CSV file for spreadsheets and other collection
  apps, one row per purchase: name, set code and name, collector number, language,
  finish, condition, quantity, price paid, acquired date, note, rarity, market price
  and Scryfall ID. Other apps can match the cards on the Scryfall ID, or on set code
  and collector number. Import reads it back.
- **Valuation report:** a page listing every card by set, newest set first, with
  finish, condition, quantity, the price of one copy and the value, a total for each
  set, and the collection's totals at the top. Open it in a browser and print it, or
  save it as a PDF, for insurance or your records. When printed, each set starts a
  new page and long tables repeat their headings.
- **Export…** under the collection list's filters exports just the cards the list
  shows, in its order, as CSV or a valuation report.
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
| Card order | The collection list's order until you choose one in its Sort menu: By name, By set (newest or oldest first), Most valuable first | By name |
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
`mtg/` holds `collection.json` (your cards), `price-history.json`, `lots.json`,
`sales.json` (sold cards), `lists.json` (lists and decks) and `backups/`.

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
taken before merging. The same goes for the lots, sales and lists files: merging adds
the lots, sales or lists only the other version has (and, on a list both have, the
cards only it has, keeping the larger number wanted). See the README for command-line options.
