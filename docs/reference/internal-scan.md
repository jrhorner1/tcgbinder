# `internal/scan`

```
package scan // import "github.com/jrhorner1/tcgbinder/internal/scan"

Package scan holds what card scanning shares across games and platforms: the
text read from a photo of a card, with where each line sits on the card. Reading
the text is platform code (package scan/vision, Apple's Vision framework);
working out which card it is belongs to each game (game.Scanner).

TYPES

type Line struct {
	Text       string  `json:"text"`
	X          float64 `json:"x"`
	Y          float64 `json:"y"`
	W          float64 `json:"w"`
	H          float64 `json:"h"`
	Confidence float64 `json:"confidence"`
}
    Line is one line of text read from a card. The box is normalized, with 0,0
    at the top-left and 1,1 at the bottom-right of the card, or of the whole
    photo when no card outline was found.

func (l Line) Bottom() float64
    Bottom is the line's lower edge.

type Reading struct {
	Lines []Line `json:"lines"`
	// CardFound reports that the card's outline was found and the photo straightened
	// to it, so positions are relative to the card itself.
	CardFound bool `json:"card_found"`
}
    Reading is the text read from one photo or camera frame.

func ParseReading(b []byte) (Reading, error)
    ParseReading decodes a reading sent as JSON by the platform code.

func (r Reading) Sorted() []Line
    Sorted returns the lines top to bottom, then left to right.

func (r Reading) Text() string
    Text is every line, top to bottom, one per row (for logs and debugging).

```
