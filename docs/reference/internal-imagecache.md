# `internal/imagecache`

```
package imagecache // import "github.com/jrhorner1/tcgbinder/internal/imagecache"

Package imagecache downloads card images and keeps recently viewed ones in
memory.

FUNCTIONS

func Fetch(url string) ([]byte, error)
    fetchImage downloads (or returns a cached copy of) the image at url.
    It blocks on the network, so call it from a goroutine, never the UI thread.

```
