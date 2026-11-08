package apple

/*
#cgo CFLAGS: -x objective-c -fobjc-arc
#cgo LDFLAGS: -framework Foundation -framework CoreImage -framework ImageIO -framework Vision -framework CoreGraphics
#include <stdlib.h>
#include "read.h"
*/
import "C"

import (
	"errors"
	"unsafe"

	"github.com/jrhorner1/tcgbinder/internal/scan"
)

// ReadAvailable reports whether ReadImage works on this platform.
func ReadAvailable() bool { return true }

// ReadImage reads a photo of a card (JPEG, PNG, HEIC, ...): finds the card,
// straightens it and reads its text. It takes a second or so; don't call it on the
// UI thread.
func ReadImage(data []byte) (scan.Reading, error) {
	if len(data) == 0 {
		return scan.Reading{}, errors.New("empty image")
	}
	p := C.CBytes(data)
	defer C.free(p)
	var cerr *C.char
	out := C.tcgb_read_image(p, C.int(len(data)), &cerr)
	if out == nil {
		msg := "text recognition failed"
		if cerr != nil {
			msg = C.GoString(cerr)
			C.free(unsafe.Pointer(cerr))
		}
		return scan.Reading{}, errors.New(msg)
	}
	defer C.free(unsafe.Pointer(out))
	return scan.ParseReading([]byte(C.GoString(out)))
}
