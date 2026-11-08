# `internal/datadir`

```
package datadir // import "github.com/jrhorner1/tcgbinder/internal/datadir"

Package datadir lays out the app's data folder: one subfolder per game, so each
game keeps its own collection.json, price-history.json and caches.

    <data>/tcgbinder.lock
    <data>/mtg/collection.json
    <data>/mtg/price-history.json
    <data>/mtg/scryfall-sets.json

CONSTANTS

const CollectionFile = "collection.json"
    CollectionFile is the name of each game's collection file.


FUNCTIONS

func DefaultRoot(appID string) (string, error)
    DefaultRoot is the desktop data folder for appID: the same folder Fyne
    uses for the app's storage, so the desktop window and --web mode share one
    collection.

        macOS    ~/Library/Preferences/fyne/<app id>
        Windows  %USERPROFILE%\AppData\Roaming\fyne\<app id>
        Linux    $XDG_CONFIG_HOME/fyne/<app id> (usually ~/.config/fyne/<app id>)

func GameDir(root, gameID string) (string, error)
    GameDir returns (creating if needed) the folder for a game's files.

```
