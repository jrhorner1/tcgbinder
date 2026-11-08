# `internal/logging`

```
package logging // import "github.com/jrhorner1/tcgbinder/internal/logging"

Package logging sets up the app's structured logging (log/slog): human-readable
lines to a log file and, optionally, stderr, with a minimum level.

CONSTANTS

const MaxSize = 10 << 20
    MaxSize is the size at which the log file is rotated to <file>.1 at startup.


FUNCTIONS

func ParseLevel(s string) (slog.Level, error)
    ParseLevel converts a level name to slog.Level.

func SetLevel(name string) error
    SetLevel changes the minimum level of the logger Setup installed.

func Setup(o Options) (closeFn func() error, err error)
    Setup installs the default slog logger (which also receives the standard log
    package's output) and returns a function that closes the log file.


TYPES

type Options struct {
	File   string // log file path; "" for no file
	Level  string // debug, info, warn or error
	Stderr bool   // also write to stderr
}
    Options configures Setup.

```
