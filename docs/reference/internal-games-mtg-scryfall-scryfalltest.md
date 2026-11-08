# `internal/games/mtg/scryfall/scryfalltest`

```
package scryfalltest // import "github.com/jrhorner1/tcgbinder/internal/games/mtg/scryfall/scryfalltest"

Package scryfalltest provides fake Scryfall API servers for tests.

FUNCTIONS

func NewCollectionServer(t *testing.T, catalog []scryfall.Card) *scryfall.Client
    fakeScryfall serves /cards/collection from a small catalog, mimicking
    Scryfall's 400-on-malformed-batch behavior and not_found reporting.

func NewSearchServer(t *testing.T) *scryfall.Client
    fakeSearch serves /cards/search: "bolt" yields 5 printings in pages of 3;
    anything else is a 404 like Scryfall's no-results answer.

```
