//go:build unix

package instance

import (
	"errors"
	"os"
	"syscall"
)

// flock locks belong to the open file, so a second open of the same file — even in
// the same process — is refused, and the OS drops the lock when the process ends.
func lockFile(f *os.File) error {
	err := syscall.Flock(int(f.Fd()), syscall.LOCK_EX|syscall.LOCK_NB)
	if errors.Is(err, syscall.EWOULDBLOCK) || errors.Is(err, syscall.EAGAIN) {
		return errLocked
	}
	return err
}

func unlockFile(f *os.File) error {
	return syscall.Flock(int(f.Fd()), syscall.LOCK_UN)
}
