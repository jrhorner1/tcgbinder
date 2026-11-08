package apple

/*
#cgo LDFLAGS: -framework UIKit -framework AVFoundation -framework PhotosUI -framework CoreMedia -framework CoreVideo -framework QuartzCore
#include <stdint.h>
#include <stdlib.h>
void tcgb_scan_start(uintptr_t handle, int photosOnly);
*/
import "C"

import (
	"errors"
	"log/slog"
	"runtime/cgo"

	"github.com/jrhorner1/tcgbinder/internal/scan"
)

// ScanAvailable reports whether this device can scan cards (iOS: always, from the
// camera or the photo library).
func ScanAvailable() bool { return true }

// scanSession is one open scanner.
type scanSession struct {
	judge func(scan.Reading) Frame
	done  ScanFunc
}

// Scan opens the camera scanner over the app. judge is called (off the UI thread)
// with each frame's reading and decides when it shows a card; done gets the
// accepted reading, an error, or ErrCancelled, on the main thread. With photosOnly,
// or on devices without a camera, the photo library opens instead.
func Scan(photosOnly bool, judge func(scan.Reading) Frame, done ScanFunc) {
	h := cgo.NewHandle(&scanSession{judge: judge, done: done})
	po := 0
	if photosOnly {
		po = 1
	}
	C.tcgb_scan_start(C.uintptr_t(h), C.int(po))
}

//export tcgbScanFrame
func tcgbScanFrame(handle C.uintptr_t, js *C.char) C.int {
	s := cgo.Handle(handle).Value().(*scanSession)
	r, err := scan.ParseReading([]byte(C.GoString(js)))
	if err != nil {
		slog.Warn("scan frame", "err", err)
		return 0
	}
	switch s.judge(r) {
	case FrameAccept:
		return 2
	case FramePartial:
		return 1
	}
	return 0
}

//export tcgbScanDone
func tcgbScanDone(handle C.uintptr_t, js *C.char, errMsg *C.char, cancelled C.int) {
	h := cgo.Handle(handle)
	s := h.Value().(*scanSession)
	h.Delete()
	switch {
	case cancelled != 0:
		s.done(scan.Reading{}, ErrCancelled)
	case errMsg != nil:
		s.done(scan.Reading{}, errors.New(C.GoString(errMsg)))
	default:
		r, err := scan.ParseReading([]byte(C.GoString(js)))
		s.done(r, err)
	}
}
