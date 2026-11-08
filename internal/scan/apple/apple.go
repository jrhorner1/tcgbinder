// Package apple reads cards with Apple's frameworks: Vision finds the card in a photo,
// straightens it and reads its text (macOS and iOS), and on iOS a camera screen scans
// cards live, with the photo library as an alternative.
package apple

import (
	"errors"

	"github.com/jrhorner1/tcgbinder/internal/scan"
)

// ErrUnavailable is returned where text recognition isn't available (not Apple).
var ErrUnavailable = errors.New("card scanning needs macOS or iOS")

// ErrCancelled is passed to a scan's done function when the user closed the scanner.
var ErrCancelled = errors.New("scan cancelled")

// Frame is what the caller makes of one camera frame's reading.
type Frame int

const (
	FrameNothing Frame = iota // nothing useful: keep looking
	FramePartial              // something readable: ask the user to hold still
	FrameAccept               // good enough: stop and return this reading
)

// ScanFunc receives the final reading of a scan, or an error (ErrCancelled if the
// user closed the scanner).
type ScanFunc func(r scan.Reading, err error)
