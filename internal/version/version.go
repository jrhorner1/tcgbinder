// Package version reports the app's version and build details.
package version

import (
	"fmt"
	"runtime"
	"runtime/debug"
)

// Version is the release version; keep it in step with FyneApp.toml. Builds can
// override it: go build -ldflags "-X github.com/jrhorner1/mtgmanager/internal/version.Version=1.2.3"
var Version = "0.1.0"

// String is the one-line version shown by --version.
func String(name string) string {
	s := fmt.Sprintf("%s %s", name, Version)
	if info, ok := debug.ReadBuildInfo(); ok {
		var rev, modified string
		for _, kv := range info.Settings {
			switch kv.Key {
			case "vcs.revision":
				rev = kv.Value
			case "vcs.modified":
				if kv.Value == "true" {
					modified = "+dirty"
				}
			}
		}
		if len(rev) > 12 {
			rev = rev[:12]
		}
		if rev != "" {
			s += fmt.Sprintf(" (%s%s)", rev, modified)
		}
	}
	return s + fmt.Sprintf(" %s %s/%s", runtime.Version(), runtime.GOOS, runtime.GOARCH)
}
