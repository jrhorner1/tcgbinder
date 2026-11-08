//go:build !darwin

package apple

import "github.com/jrhorner1/tcgbinder/internal/scan"

// ReadAvailable reports whether ReadImage works on this platform.
func ReadAvailable() bool { return false }

// ReadImage needs Apple's Vision framework.
func ReadImage([]byte) (scan.Reading, error) { return scan.Reading{}, ErrUnavailable }
