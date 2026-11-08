package instance

import (
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
)

func TestSecondAcquireFails(t *testing.T) {
	dir := t.TempDir()
	first, err := Acquire(dir)
	if err != nil {
		t.Fatal(err)
	}
	_, err = Acquire(dir)
	if !errors.Is(err, ErrRunning) {
		t.Fatalf("second acquire: %v, want ErrRunning", err)
	}
	if !strings.Contains(err.Error(), strconv.Itoa(os.Getpid())) {
		t.Errorf("error should name the owning process: %v", err)
	}
	if err := first.Release(); err != nil {
		t.Fatal(err)
	}
	again, err := Acquire(dir)
	if err != nil {
		t.Fatalf("after release: %v", err)
	}
	again.Release()
}

func TestSeparateFoldersDontConflict(t *testing.T) {
	a, err := Acquire(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	defer a.Release()
	b, err := Acquire(t.TempDir())
	if err != nil {
		t.Fatalf("a different data folder should get its own lock: %v", err)
	}
	b.Release()
}

// TestLockAcrossProcesses holds the lock in a child process and checks the parent
// is refused, then that the lock frees once the child exits (as after a crash).
func TestLockAcrossProcesses(t *testing.T) {
	if dir := os.Getenv("INSTANCE_HOLDER_DIR"); dir != "" {
		l, err := Acquire(dir)
		if err != nil {
			os.Exit(2)
		}
		os.Stdout.WriteString("locked\n")
		var b [1]byte
		os.Stdin.Read(b[:]) // hold until the parent closes stdin
		_ = l
		os.Exit(0) // exit without Release: the OS must drop the lock
	}

	dir := t.TempDir()
	cmd := exec.Command(os.Args[0], "-test.run=TestLockAcrossProcesses")
	cmd.Env = append(os.Environ(), "INSTANCE_HOLDER_DIR="+dir)
	stdin, _ := cmd.StdinPipe()
	stdout, _ := cmd.StdoutPipe()
	if err := cmd.Start(); err != nil {
		t.Fatal(err)
	}
	buf := make([]byte, 7)
	if _, err := stdout.Read(buf); err != nil || string(buf[:6]) != "locked" {
		t.Fatalf("child did not take the lock: %q %v", buf, err)
	}

	_, err := Acquire(dir)
	if !errors.Is(err, ErrRunning) || !strings.Contains(err.Error(), strconv.Itoa(cmd.Process.Pid)) {
		t.Fatalf("parent acquire while child holds lock: %v", err)
	}

	stdin.Close()
	cmd.Wait()
	l, err := Acquire(dir)
	if err != nil {
		t.Fatalf("lock should be free after the holder exits: %v", err)
	}
	l.Release()
	if _, err := os.Stat(filepath.Join(dir, LockFileName)); err != nil {
		t.Errorf("lock file should remain for reuse: %v", err)
	}
}
