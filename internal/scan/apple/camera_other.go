//go:build !ios

package apple

import "github.com/jrhorner1/tcgbinder/internal/scan"

// ScanAvailable reports whether this device can scan cards with a camera (iOS only).
func ScanAvailable() bool { return false }

// Scan is only available on iOS; elsewhere done gets ErrUnavailable.
func Scan(photosOnly bool, judge func(scan.Reading) Frame, done ScanFunc) {
	done(scan.Reading{}, ErrUnavailable)
}
