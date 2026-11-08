# `internal/version`

```
package version // import "github.com/jrhorner1/tcgbinder/internal/version"

Package version reports the app's version and build details.

VARIABLES

var Version = "0.1.0"
    Version is the release version; keep it in step with
    FyneApp.toml. Builds can override it: go build -ldflags "-X
    github.com/jrhorner1/tcgbinder/internal/version.Version=1.2.3"


FUNCTIONS

func String(name string) string
    String is the one-line version shown by --version.

```
