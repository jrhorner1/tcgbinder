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

## Collection

- **Browse:** every card is listed with its set, number and copies, in name order, by
  set, or most valuable first, as **Settings → Card order** says.
- **Filter:** search by name, set or collector number, and filter by set, color,
  rarity and finish. The line under the filters counts unique cards, cards and sets;
  **Clear filters** shows everything again.
- **Set pages:** choose a set in the **Set** filter, or click a set on Home. They show
  completion, value, the most valuable cards (ranked by one copy), and **Missing
  Cards**. That list is loaded from Scryfall on request; click a missing card to add
  it.

On a wide screen the selected card or set shows beside the list. On a phone (or a
narrow window) a card replaces the list, with a **Back** button.

### Editing a card

Each printing you own appears once. Its copies are grouped by **finish**, then
**condition**, and each condition keeps a list of **purchases**: how many copies,
the price paid for each, and the date acquired. For example, two near-mint normal
copies bought for $0.10 and $0.12 are two purchases, so price paid and value over
time stay exact.

- **Your Copies** lists one row per purchase: finish, condition, quantity, price
  paid (each) and acquired date. **Add purchase** adds a row; ✕ removes one. Setting
  every quantity to 0 removes the card when you save.
  - The finish menu lists only the finishes that printing comes in ("Normal",
    "Foil", "Etched").
  - Purchases with the same finish, condition, price and date are combined.
- **Change printing…** moves every copy to another printing of the same card (for
  example, a different set) without deleting and re-adding it. Each purchase keeps
  its condition, price paid and date; a finish the new printing doesn't come in
  becomes its first finish.
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
   paid or acquired date for every copy, or delete them. You can still untick cards
   there. Finish changes skip printings that don't come in that finish, and copies
   that end up identical join one purchase.

The ticks clear once the changes are applied. **Tools → Bulk edit…** opens the same
dialog.

## Adding cards

**Add** searches Scryfall by name, set code or both, and lists every printing (cards
you already own say so). Pick the exact one, then set the quantity, finish,
condition, price paid and date. Adding a printing you already own adds a purchase to
it (or to a matching purchase with the same finish, condition, price and date).

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
**Search by hand** helps when the card was misread.

Tips: good light without glare on the name and the bottom-left corner, and the whole
card in view. Scanning can't tell foil from non-foil, so set the finish yourself.

## Search

The search box at the top searches your whole collection, across every game. If the
card isn't there, **Search the catalog** opens Add with the same search.

## Tools

The Tools page works on the selected game:
- **Import…:** CSV exports from EchoMTG, ManaBox, Moxfield, Deckbox, TCGplayer,
  Dragon Shield, Archidekt and others, or a collection `.json` exported from this app.
  Cards you already own gain the imported purchases.
- **Export collection:** saves the collection file, which Import accepts. Use it for
  backups and moving to another device. The desktop app saves it to your Downloads
  folder; the iPhone app saves it to its folder in the Files app; the web app
  downloads it.
- **Bulk edit…** (see above) and **Scan a card…**.
- **Refresh prices now** and **Load/Update price history**, with when each last ran.

## Settings

Changes save as you make them and apply to every game. They're stored in
`settings.json` in the data folder and shared by the desktop and web apps.

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

Settings also shows the data folder, the version, and **Restore defaults**.

## Where your data lives

Each game has its own folder in the app's data folder:

| Platform | Data folder |
|---|---|
| macOS | `~/Library/Application Support/io.github.jrhorner1.tcgbinder` |
| Windows | `%APPDATA%\io.github.jrhorner1.tcgbinder` |
| Linux | `~/.local/share/io.github.jrhorner1.tcgbinder` |
| iPhone, iPad | the app's private storage |

Inside it, `mtg/` holds `collection.json` (your cards),
`price-history.json` and caches; the log file `tcgbinder.log` and `settings.json`
sit in the data folder itself. See the README for command-line options.
