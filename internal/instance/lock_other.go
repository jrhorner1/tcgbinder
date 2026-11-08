//go:build !unix && !windows

package instance

import "os"

// Platforms without file locks (e.g. the browser build) run one copy per page anyway.
func lockFile(*os.File) error   { return nil }
func unlockFile(*os.File) error { return nil }
