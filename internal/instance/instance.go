// Package instance makes sure only one copy of the app uses a data folder at a time,
// using an OS file lock that is released automatically if the app exits or crashes.
package instance

import (
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strconv"
	"strings"
)

// ErrRunning means another process holds the lock.
var ErrRunning = errors.New("another copy of the app is already running")

// LockFileName is created in the data folder; it holds the owner's process ID.
const LockFileName = "tcgbinder.lock"

// Lock is a held single-instance lock. Keep it for the life of the process.
type Lock struct {
	f *os.File
}

// Acquire takes the lock for dir without waiting. If another process holds it, the
// returned error wraps ErrRunning and names that process's ID when known.
func Acquire(dir string) (*Lock, error) {
	path := filepath.Join(dir, LockFileName)
	// No O_TRUNC: the PID inside belongs to the current owner until we hold the lock.
	f, err := os.OpenFile(path, os.O_RDWR|os.O_CREATE, 0o600)
	if err != nil {
		return nil, err
	}
	if err := lockFile(f); err != nil {
		owner := readPID(f)
		f.Close()
		if errors.Is(err, errLocked) {
			if owner > 0 {
				return nil, fmt.Errorf("%w (process %d)", ErrRunning, owner)
			}
			return nil, ErrRunning
		}
		return nil, err
	}
	if err := f.Truncate(0); err == nil {
		_, _ = f.WriteAt([]byte(strconv.Itoa(os.Getpid())+"\n"), 0)
		_ = f.Sync()
	}
	return &Lock{f: f}, nil
}

// Release drops the lock. The lock file itself is left in place; it is reused next run.
func (l *Lock) Release() error {
	if l == nil || l.f == nil {
		return nil
	}
	_ = unlockFile(l.f)
	err := l.f.Close()
	l.f = nil
	return err
}

func readPID(f *os.File) int {
	b, err := io.ReadAll(io.NewSectionReader(f, 0, 32))
	if err != nil {
		return 0
	}
	pid, _ := strconv.Atoi(strings.TrimSpace(string(b)))
	return pid
}

var errLocked = errors.New("locked")
