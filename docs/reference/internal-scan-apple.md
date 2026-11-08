# `internal/scan/apple`

```
package apple // import "github.com/jrhorner1/tcgbinder/internal/scan/apple"

Package apple reads cards with Apple's frameworks: Vision finds the card in a
photo, straightens it and reads its text (macOS and iOS), and on iOS a camera
screen scans cards live, with the photo library as an alternative.

VARIABLES

var ErrCancelled = errors.New("scan cancelled")
    ErrCancelled is passed to a scan's done function when the user closed the
    scanner.

var ErrUnavailable = errors.New("card scanning needs macOS or iOS")
    ErrUnavailable is returned where text recognition isn't available (not
    Apple).


FUNCTIONS

func ReadAvailable() bool
    ReadAvailable reports whether ReadImage works on this platform.

func ReadImage(data []byte) (scan.Reading, error)
    ReadImage reads a photo of a card (JPEG, PNG, HEIC, ...): finds the card,
    straightens it and reads its text. It takes a second or so; don't call it on
    the UI thread.

func Scan(photosOnly bool, judge func(scan.Reading) Frame, done ScanFunc)
    Scan is only available on iOS; elsewhere done gets ErrUnavailable.

func ScanAvailable() bool
    ScanAvailable reports whether this device can scan cards with a camera (iOS
    only).


TYPES

type Frame int
    Frame is what the caller makes of one camera frame's reading.

const (
	FrameNothing Frame = iota // nothing useful: keep looking
	FramePartial              // something readable: ask the user to hold still
	FrameAccept               // good enough: stop and return this reading
)
type ScanFunc func(r scan.Reading, err error)
    ScanFunc receives the final reading of a scan, or an error (ErrCancelled if
    the user closed the scanner).

```
