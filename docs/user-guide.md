# User guide

TCG Binder keeps your trading card collection on your own device, looks cards up on
Scryfall, and tracks what you paid, what it's worth, and how complete your sets are.
The same app runs on the desktop (macOS, Windows, Linux), on iPhone and iPad, and in a
web browser (`tcgbinder --web`), with the same pages everywhere.

On the desktop (and in a wide browser window) the sidebar has **Home**, your
collection (**Magic**), **Statistics**, **Lists**, **Bundles/Lots**, **Tools** and
**Settings**, with **Add cards** (a menu: a single card, or a pack, bundle or lot)
and **Scan a card** below them. On phones the tab bar at the bottom has **Home**, **Cards**,
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
- **Set Completion:** distinct collector numbers owned out of each set's size: by
  default the printed set, as the number on the cards says ("123/280": Core Set 2019
  has 280, though Scryfall lists 314 with its variants and extras). Only plain numbers
  up to that size count. **Settings → Set completion** can count every card instead.
  Sets without a printed size count every card. Click a
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
  Cards**. That list is loaded from Scryfall on request and follows the Set completion
  setting. It says what one of each missing card would cost at market price, sorts
  by number or most expensive first, and **Copy to a list…** puts them all on a list
  (a new one, or one you have). Click a missing card to add it.

On a wide screen the selected card or set shows beside the list. On a phone (or a
narrow window) a card replaces the list, with a **Back** button, which returns to
the same place in the list. Selecting a card keeps the list where it was scrolled.
When the card has little room, its picture sits above the details and each purchase
takes two lines. In the desktop and iPhone apps, **View on Scryfall** and other links
to websites open in your browser.

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
   notes**), tags (**Add tags**, **Remove tags**, or **Remove all tags**; see Tags),
   or delete them. You can still untick cards
   there. Finish changes skip printings that don't come in that finish, and copies
   that end up identical join one purchase.

The ticks clear once the changes are applied. The status line then offers **Undo**,
which puts the collection back as it was, as long as nothing else has changed it
since; otherwise use **Tools → Restore a backup…**. Imports offer Undo the same way.

**Tools → Bulk edit…** opens the same dialog.

### Tags

Tags are your own labels on the copies you own, such as "binder A", "elves deck",
"for sale" or "to grade", to filter, sort and total the collection by. A tag belongs
to a purchase, so four copies of a card in a deck and two in a binder can be tagged
differently. There is no list of tags to keep: the tags that exist are the ones on
your cards, and a tag disappears when it is removed from its last card.

- **Adding tags:** on a card's page, **Add tags** on a purchase opens a tag box. Type
  a tag and press Enter (or type a comma or semicolon); Backspace in the empty box
  removes the last one, and × removes any. Tags you already use are suggested. A tag
  can be up to 40 characters, without commas or semicolons, and can't start with
  `lot:` (the app keeps lots that way), and a purchase can have up to 20. Tags ignore case: "Binder A" and "binder a" are one tag, kept in the
  spelling first used. To tag some copies of a purchase apart from the rest,
  **Split** moves one copy to a row of its own; set the quantities, change its tags,
  and save. The Add page has a tag box too, and keeps its tags for the next card, so
  a stack going to one place is tagged once. **Bulk edit** adds or removes tags on
  many cards at once.
- **In the list:** each card shows up to three of its tags (and "+N" for more);
  click one to show the cards with that tag. **More filters → Tags** lists every tag
  with its number of cards: click a tag to show cards with it, again to show cards
  without it (struck through), a third time to clear it. With two or more chosen,
  choose **All of these** or **Any of these**. **Untagged** shows cards with a copy
  that has no tags. The search box finds tags too, and **Sort: Tag** orders cards by
  their first tag, untagged cards last.
- **Tools → Tags…** lists every tag with how many cards and copies carry it, what
  they're worth and what they cost (counting only the tagged copies), and the
  untagged copies. **Rename…** changes a tag everywhere; renaming it to a tag you
  already have combines the two (the app asks first). **Delete…** removes a tag from
  every card. Both offer Undo, and a backup is taken first.
- **Statistics** has a Tags table with the same totals.
- **Import and export:** an import preview can tag every card in the import. The CSV
  export has a Tags column (tags joined with "; "), and imported files with a Tags,
  Tag, Labels or Label column bring their tags in; a tag that can't be used is
  skipped and the preview says how many. Selling copies remembers their tags, and
  undoing the sale puts them back with them.

## Adding cards

**Add cards** in the sidebar offers **A single card** (the Add page) or **A pack,
bundle or lot…** (see below); on a phone, the Add tab opens the Add page, which offers
both. **Add** searches Scryfall by name, set code or both, and lists every printing, newest
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
  under **Added this session**, beside the add form (below it on a phone, so the
  results keep their room), each with **Undo**, which takes back exactly those
  copies.
- **Keyboard:** Enter searches; ↑ and ↓ move through the results (or the
  suggestions); Enter then adds the selected printing.
- **Paste a list…** takes a card list, one card per line, as decklists from
  Moxfield, Archidekt or MTG Arena write them: `4 Lightning Bolt`,
  `4x Lightning Bolt`, `4 Lightning Bolt (M11) 149`, `4 Lightning Bolt [M11] 149`, with
  `*F*` or `(foil)` at the end for foil. Headings such as "Sideboard" and comments
  (`//`, `#`) are skipped. It shows the same preview as an import before anything is
  saved. Import accepts such a list as a file too.
- **Missing cards:** on a set page, **Add…** beside a missing card adds it without
  leaving the page.

## Adding a pack, bundle or lot

When you buy cards together for one price (a pack, usually 14 or 15 cards; a bundle;
a bulk lot), **A pack, bundle or lot…** records the purchase once (from the sidebar's
**Add cards** menu, the Add page, or **Add a pack, bundle or lot…** on the
Bundles/Lots page; they're all the same to the app) and works out each card's price for you:
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
data folder. While you enter a lot, its banner links to the lot's page and to all
lots. **Bundles/Lots** (in the sidebar; on a phone, **Tools → Bundles/Lots…**) lists
every lot with what it cost, how many of its cards are entered ("37 of 500
entered"), and what its cards are worth now; **Add a pack, bundle or lot…** there starts a new one
and goes on to the Add page to enter its cards. A lot's page lists its cards, with
**Edit** (changing the total, count, date or split reprices the cards), **Continue
adding**, **Add cards you own…**, **Recalculate at today's prices** (finished
value-split lots) and **Delete** (keep the cards with their prices, or delete them
too).

- **Cards you already own** can join a lot, for a purchase you entered card by card
  before. **Add cards you own…** on the lot's page searches your collection: choose
  how many copies of a card to put in. Or tick cards in the collection list and use
  **Bulk edit → Put in a lot** to put every copy of them in. Either way the copies
  take the lot's date and their share of its price (the lot is repriced), copies that
  were in another lot leave it, and Undo puts things back.
- **Remove from lot** beside a card on the lot's page takes it out of the lot without
  removing it from your collection: its copies keep the price and date the lot gave
  them, and the lot's other cards are repriced. On a card's page, copies from a lot name it; a price typed there
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

## Statistics

**Statistics** (in the sidebar; on a phone, **Tools → Statistics…**) breaks the
collection down:

- **By price of one copy:** under $1, $1 to $5, $5 to $20, $20 to $100, and $100 and
  up (plus cards with no price), each with its copies, market value and share of the
  collection's value.
- **Rarity, finish, condition, language, release year, color and card type:** copies
  and market value for each. A card counts under each color and type it has, plus
  Multicolor for two or more colors. Finish and condition count the copies in each.
- **By month acquired:** copies and money spent, newest first (the last 12 months,
  with **Show all**). Copies without an acquired date are left out, and the page
  says how many.
- **Most copies:** the 10 cards you have the most copies of.

Click a row (all but release year) to see those cards in the collection list, with
the matching filter.

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

## Watchlist and price alerts

**Watch…** on the Add page's card form, or **Watch price…** on a card you own,
watches a card's price in one finish. Choose when to be alerted:

- **It moves by (%)**: a rise or fall of that percentage from the price when you
  started watching (or last reset);
- **It reaches or passes ($)**, or **It falls to or under ($)**: a price line.

Prices are checked when the app refreshes them (each day, or as **Settings → Update
prices** says), so alerts appear while the app is open or the next time you open
it. There is no email or text message. An alert fires when a refresh takes the price
past the line; a rule the price already meets fires the next time it crosses.

Alerts show in a **Price alerts** box on Home, and the desktop and iPhone apps also
post a system notification. **Dismiss** hides an alert, keeping the starting price:
it fires again only if the price comes back and crosses again. **Reset** measures
from today's price from now on. **Tools → Watchlist…** lists every watched card with
its starting price, today's price, the change and the alert, with **Change…**,
**Reset** and ✕ (stop watching).

## Sealed product

**Tools → Sealed product…** keeps boxes, bundles and decks you hold unopened. No free
source prices sealed product, so you enter each item's value yourself.

**Add sealed product** asks for a set code; **Find products** lists that set's
products (from MTGJSON) to choose from, or you can type a name. Then the quantity,
the price paid each, the date, the value each and a note. A value is dated the day
you enter it, and values older than 90 days (or not entered yet) are marked so you
know to look them up again. **Look up a price ↗** opens a shop's page for the
product when MTGJSON has one.

The page totals the items, what you paid and what they're worth. Its gain/loss
covers the items with both a price paid and a value. Sealed product isn't part of the
cards' market value or the value chart on Home; Home adds a line under Market Value,
"plus $X sealed", that links here.

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
  finish, condition, quantity, price paid, acquired date, note, tags, rarity, market
  price and Scryfall ID. Other apps can match the cards on the Scryfall ID, or on set code
  and collector number. Import reads it back.
- **Valuation report:** a page listing every card by set, newest set first, with
  finish, condition, quantity, the price of one copy and the value, a total for each
  set, and the collection's totals at the top. Open it in a browser and print it, or
  save it as a PDF, for insurance or your records. When printed, each set starts a
  new page and long tables repeat their headings.
- **Export…** under the collection list's filters exports just the cards the list
  shows, in its order, as CSV or a valuation report.
- **Bulk edit…** (see above) and **Scan a card…**.
- **Tags…:** every tag, with Rename and Delete (see Tags).
- **Restore a backup…:** a copy of the collection is saved before every import,
  bulk edit, bulk delete, restore, tag rename and tag delete, and the last 10 are
  kept. The list shows when
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

**Tag every card in this import** puts tags on everything the import adds (optional).
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
| Set completion | What a set's completion counts: the printed set (collector numbers up to the size printed on the cards), or every card Scryfall lists, variants too | The printed set |
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
`sales.json` (sold cards), `lists.json` (lists and decks), `watchlist.json`,
`sealed.json` and `backups/`.

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
taken before merging. The same goes for the lots, sales, lists, watchlist and sealed
files: merging adds the lots, sales, lists, watches or sealed items only the other
version has (on a list
both have, the cards only it has, keeping the larger number wanted; for a card both
watch, the one reset more recently). See the README for command-line options.
