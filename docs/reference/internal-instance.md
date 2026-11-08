# `internal/instance`

```
package instance // import "github.com/jrhorner1/tcgbinder/internal/instance"

Package instance makes sure only one copy of the app uses a data folder at a
time, using an OS file lock that is released automatically if the app exits or
crashes.

CONSTANTS

const LockFileName = "tcgbinder.lock"
    LockFileName is created in the data folder; it holds the owner's process ID.


VARIABLES

var ErrRunning = errors.New("another copy of the app is already running")
    ErrRunning means another process holds the lock.


TYPES

type Lock struct {
	// Has unexported fields.
}
    Lock is a held single-instance lock. Keep it for the life of the process.

func Acquire(dir string) (*Lock, error)
    Acquire takes the lock for dir without waiting. If another process holds it,
    the returned error wraps ErrRunning and names that process's ID when known.

func (l *Lock) Release() error
    Release drops the lock. The lock file itself is left in place; it is reused
    next run.

```
