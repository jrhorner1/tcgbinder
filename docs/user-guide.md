# User guide

TCG Binder keeps your trading card collection on your own device, looks cards up on
Scryfall, and tracks what you paid, what it's worth, and how complete your sets are.
The same app runs on the desktop (macOS, Windows, Linux), on iPhone and iPad, and in a
web browser (`tcgbinder --web`), with the same pages everywhere.

On the desktop (and in a wide browser window) the sidebar has **Home**, your
collection (**Magic**), **Statistics**, **Lists**, **Tools** and **Settings**, with
**Add…** and **Scan a card** below them. On phones the tab bar at the bottom has
**Home**, **Cards**, **Add**, **Lists**, **Tools** and **Settings**. The search box at the top of
every page searches your cards and the catalog (see Search).

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

- **Browse:** every card is a line: its picture, name, set, number and rarity, the
  lots its copies came from, how many copies you have and what they're worth. Under
  it, its copies are listed in groups (copies that are the same in every way: finish,
  condition, price paid, date, lot and tags), each with its actions on the same row:
  - **− / +** takes a copy away or adds one; taking the last one away removes the
    group. The finish and condition menus change the group.
  - The price paid each, the date and what one copy is worth; a 📦 badge names the
    lot the copies came from (click it to see that lot's cards, keeping the other
    filters); tag chips (click a tag's
    name to add it to the filters, so clicking two tags shows the cards with both;
    its **×** takes the tag off) and **+ tag** to
    add one.
  - **Put in lot…** (or **Move…** for copies already in a lot) puts the group in a
    lot, or takes it out: in a lot, copies take its date and their share of its
    price, and the lot is repriced.
  - **Sell…** records a sale of some of the group's copies right there (see Selling
    cards).
  - **Split** moves one copy into a group of its own, to tag it or change its
    condition apart from the rest. Pressing it again moves another copy into that
    same group. Once the new group is changed, it's kept apart, and the next split
    starts another group. A split that's never changed goes back with the others
    when the app is reopened, since identical copies are kept together.
  - **Delete** removes the group's copies, after asking.

  Every change is saved straight away. A card's name, picture or **Details…** opens
  its page, for changing its printing, watching its price, or editing it in full.
  More cards load by themselves as you scroll toward the end of the list.
- **Sort:** the **Sort** menu above the list orders by name, set (release date, then
  collector number), number, price of one copy, value of every copy, gain or loss
  (cards with no price paid last), price paid, quantity, acquired date (the earliest
  purchase; undated cards last), release date, or rarity. The arrow beside it turns
  the order around. Choosing a sort starts with the most first, except for name and
  number. Until you choose one, the list follows **Settings → Card order**.
- **Filter:** filter the list by name, set, collector number or tag, and by set,
  color, rarity, finish and **Bought in** (a lot, or **No lot** for copies bought on
  their own). With **Bought in** or a tag chosen (or a tag excluded, or **Untagged**),
  each card shows only its copies that match (bought that way, carrying the tag), and
  the counts and value count only those; a note under a card says how many of its
  other copies are hidden. **More filters** adds color identity, mana value, condition (any copy in
  it), language, type ("creature" matches every type line with it), price of one copy
  (from and to), acquired date (any purchase in the range), a minimum number of
  copies, and cards that have a copy with no price paid, or a copy with no market
  price. Each filter in use shows as a chip
  under the filters; click the chip to remove it.
- The line under the filters counts unique cards, cards and sets, and adds up the
  market value and price paid of the cards shown. **Clear filters** shows everything
  again and keeps the sort. Sort and filters are part of the page's address, so a
  bookmark keeps them.
- **Set pages:** choose a set in the **Set** filter, or click a set on Home. Above the
  set's cards, they show its completion and value, with two buttons:
  - **View the N missing cards** opens a page of the cards you don't own yet, loaded
    from Scryfall and following the Set completion setting. It says what one of each
    would cost at market price, sorts by number or most expensive first, and has
    **To list…** and **Add…** beside each card (click a card to search its
    printings).
  - **Create a list of missing cards…** (also on that page) asks for the list's name,
    "Core Set 2019 Missing" to start with, and makes a list with one of each missing
    card, then opens it.

A card's page replaces the list, with a **Back** button, which returns to the list.
On a phone each group of copies takes three lines: the quantity, finish and
condition; the price, lot and tags; and the actions. In the desktop and iPhone apps, **View on Scryfall** and other links
to websites open in your browser.

### Editing a card

Each printing you own appears once. Its copies are grouped by **finish**, then
**condition**, and each condition keeps a list of **purchases**: how many copies,
the price paid for each, the date acquired, and its tags. For example, two near-mint normal
copies bought for $0.10 and $0.12 are two purchases, so price paid and value over
time stay exact.

- **Your Copies** on the card's page lists one row per purchase: finish, condition,
  quantity, price paid (each) and acquired date, with **Add tags** beneath. **Add
  purchase** adds a row; ✕ removes one. Setting every quantity to 0 removes the card
  when you save.
  - The finish menu lists only the finishes that printing comes in ("Normal",
    "Foil", "Etched").
  - Purchases with the same finish, condition, price, date and tags are combined;
    different tags keep them apart.
- **Language** is shown on every card. It belongs to the printing, so to change it,
  use **Change printing…**.
- **Change printing…** moves every copy to another printing of the same card (for
  example, a different set or language) without deleting and re-adding it. The
  **Language** menu shows the printings in that language (the card's own at
  first). Each purchase keeps its condition, price paid, date and tags; a finish the
  new printing doesn't come in becomes its first finish.
- **Raw data** shows the card exactly as stored, as JSON.
- **Delete** removes every copy, after asking.

### Bulk edit

Every card in the collection list has a tick box:
1. Narrow the list with the search box and the **Set**, **Color**, **Rarity** and
   **Finish** filters.
2. Tick cards one by one, or **Select all N** to tick everything the filters
   show. Ticks stay while you change filters, so you can build up a selection from
   several searches; the bar above the list (it stays in view as you scroll) says how many are ticked and how many
   the filters hide. **Clear** unticks everything.
3. **Bulk edit…** opens the changes for the ticked cards: condition, finish, price
   paid or acquired date for every copy, tags (**Add tags**, **Remove tags**, or **Remove all tags**; see Tags),
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

**Add…** in the sidebar (on a phone, the **Add** tab) opens the Add dialog, ready to
add a card. Set the price paid each, the date and any tags once at the top; they're
used for every card you add until you change them. Then search the catalog by name,
or a name and set code ("Shock M19"; a set code is the last word, with a digit or in
capitals). Every printing is listed, with what it's worth and how many you own; set
the quantity, finish and condition on a printing's row and press **Add**. The search
stays, so the next copy is one click away, and what you've added is listed under
**Added just now**. **Done** closes the dialog and shows the collection.

- **Searching:** a name is searched as a phrase, so "sol ring" finds Sol Ring and not
  every card with "sol" and "ring" somewhere in its name. If the phrase finds
  nothing, the words are searched anywhere in a name. Scryfall's search syntax (such
  as `t:goblin cmc<=2`, or a name in quotes) is used as typed.
- **Adding a printing you already own** adds a purchase to it (or to a matching
  purchase with the same finish, condition, price, date and tags).
- **Paste a list…** takes a card list, one card per line, as decklists from
  Moxfield, Archidekt or MTG Arena write them: `4 Lightning Bolt`,
  `4x Lightning Bolt`, `4 Lightning Bolt (M11) 149`, `4 Lightning Bolt [M11] 149`, with
  `*F*` or `(foil)` at the end for foil. Headings such as "Sideboard" and comments
  (`//`, `#`) are skipped. It shows the same preview as an import before anything is
  saved. Import accepts such a list as a file too.
- **From the search at the top:** a catalog card's **Add…** opens the dialog on that
  printing.
- **Missing cards:** on a set's missing cards page, **Add…** beside a card adds it
  without leaving the page.

## Adding a pack, bundle or lot

When you buy cards together for one price (a booster pack or box, a bundle, a deck
builder's toolkit, an eBay lot), record the purchase once and the app works out each
card's price. In the Add dialog, press **📦 A pack, bundle or lot…** beside the
title; the **×** beside it goes back to adding a card. The form starts blank:
- **Fill in:** small buttons (Booster pack, Booster box, Collector booster box,
  Bundle, Prerelease kit, Deck builder's toolkit) fill in what that product usually
  holds, and the name if it's empty. Change anything that's different.
- **Name, total paid and date bought.** Every card in the lot gets the date.
- **What's in it:** packs × cards per pack (14 if left blank), other cards (a promo,
  a toolkit's cards, or the count of a bulk lot), and basic lands. Basic lands don't
  take a share of the cost.
- **Split the cost:**
  - **By each card's market value** (the default): a card worth more costs more. If
    the lot's cards are worth $80 and it cost $55, each card is costed at 68.75% of
    its market price. A card without a price is valued at the average of the lot's
    cards of its rarity.
  - **Evenly:** every card costs the same, kept to four decimal places so the lot
    adds up to its total ($38 for 500 cards is $0.0760 each).

**Start entering cards** keeps the dialog open on the lot: it shows how many cards are
entered ("37 of 504 entered") and what each costs so far. Search and add the lot's
cards as above (or **Paste a list…**); they belong to the lot, which sets their price
and date. Until it's finished they carry an even share. **Pause** closes the dialog;
**Finish** works out a value split from that day's prices (refreshing older ones
first), showing what each card will be costed at before saving. If you entered fewer
cards than counted, it asks whether to spread the total over the cards entered.

An unfinished lot shows as a banner above the collection, on any device sharing the
data folder, with **Keep adding** and **Show its cards**. Choosing a lot in the
collection's **Bought in** filter (or clicking a 📦 badge) shows its cards with the
lot's figures above them: what it cost, what its cards are worth now, how many are
entered, and the gain or loss, with **Keep adding** (or **Add more** for a finished
lot), **Finish**, **Reprice at today's prices** (finished value-split lots), **Edit**
(changing the total, count, date or split reprices the cards) and **Delete** (keep
the cards with their prices, or delete them too).

- **Cards you already own** can join a lot: **Put in lot…** on a group of copies in
  the collection, or tick cards and use **Bulk edit → Put in a lot** to put every
  copy of them in. Either way the copies take the lot's date and their share of its
  price (the lot is repriced), and copies that were in another lot leave it.
- **Taking copies out:** **Move…** on a group, then **No lot**. The copies keep the
  price and date the lot gave them, and the lot's other cards are repriced. Deleting
  a lot's copies reprices it too.

## Selling cards

To record a sale, choose **Sell…** on the group of copies in the collection (say how
many and what each sold for, and any fees), or open the card and choose **Sell…**
under the purchase the copies came from for the full form. Say how many copies, what each sold for, the date, any fees for the whole
sale, and a note (where or to whom). The dialog shows the gain as you type: what they
sold for, less fees and what you paid. The copies leave your collection (a card with
none left leaves the list), and the sale is kept even after the card is gone.

The earnings page (`#/earnings`; no longer linked from Tools) lists your sales, newest first, for all years or one year: what
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

**Lists** (in the sidebar, and in a phone's tab bar) keeps decks and want
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

**Scan a card** (in the Add dialog, and in the sidebar) identifies a physical card.

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

The search box at the top of every page searches as you type: **In your collection**
lists your cards that match (click one to open it; **Show all N** opens the collection
filtered to the search), and **In the catalog** lists matching printings from
Scryfall, each with **Add…**, which opens the Add dialog on it. Press **/** anywhere
to jump to the box, and Escape to close the results. Enter shows the full search page:
your whole collection across every game, the first 200 matches per game, with **Search
the catalog** to add a card that isn't there.

## Tools

The Tools page works on the selected game:
- **Import…:** CSV exports from EchoMTG, ManaBox, Moxfield, Deckbox, TCGplayer,
  Dragon Shield, Archidekt and others, this app's own CSV, or a collection `.json`
  exported from this app.
  Cards you already own gain the imported purchases. See **Importing** below.
- **Export…:** asks which kind of file, then saves the whole collection. The desktop
  app saves it to your Downloads folder; the iPhone app saves it to its folder in the
  Files app; the web app downloads it.
  - **Collection file:** the collection exactly, which Import accepts. Use it for
    backups and moving to another device.
  - **CSV:** every card for spreadsheets and other collection apps, one row per
    purchase: name, set code and name, collector number, language,
    finish, condition, quantity, price paid, acquired date, tags, rarity, market
    price and Scryfall ID. Other apps can match the cards on the Scryfall ID, or on
    set code and collector number. Import reads it back.
- **Export…** under the collection list's filters exports just the cards the list
  shows, in its order, as CSV or as a valuation report (a page listing the cards by
  set with their value, to print or save as a PDF).
- **Tags…:** every tag, with Rename and Delete (see Tags).
- **Restore a backup…:** a copy of the collection is saved before every import,
  bulk edit, bulk delete, restore, tag rename and tag delete, and when deleting all
  data if you choose to keep one; the last 10 are
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

**Delete all data…**, under the data folder, deletes your whole collection and
everything with it: lots, sales, lists, the watchlist, sealed product and price
history, for every game. Your settings stay. It asks first: type **delete** to
confirm. **Keep a backup of the collection** (ticked at first) backs the collection up
before deleting, so **Tools → Restore a backup…** can bring it back; lots, sales,
lists, the watchlist and sealed product aren't in the backup. Unticked, the backups
are deleted too, and nothing can be brought back.

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
