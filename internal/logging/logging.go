// Package logging sets up the app's structured logging (log/slog): human-readable
// lines to a log file and, optionally, stderr, with a minimum level.
package logging

import (
	"fmt"
	"io"
	"log"
	"log/slog"
	"os"
	"path/filepath"
	"strings"
)

// MaxSize is the size at which the log file is rotated to <file>.1 at startup.
const MaxSize = 10 << 20

// Options configures Setup.
type Options struct {
	File   string // log file path; "" for no file
	Level  string // debug, info, warn or error
	Stderr bool   // also write to stderr
}

// ParseLevel converts a level name to slog.Level.
func ParseLevel(s string) (slog.Level, error) {
	switch strings.ToLower(strings.TrimSpace(s)) {
	case "debug":
		return slog.LevelDebug, nil
	case "", "info":
		return slog.LevelInfo, nil
	case "warn", "warning":
		return slog.LevelWarn, nil
	case "error":
		return slog.LevelError, nil
	}
	return 0, fmt.Errorf("unknown log level %q (use debug, info, warn or error)", s)
}

// level is the logger's minimum level; SetLevel changes it while the app runs.
var level = new(slog.LevelVar)

// SetLevel changes the minimum level of the logger Setup installed.
func SetLevel(name string) error {
	l, err := ParseLevel(name)
	if err != nil {
		return err
	}
	level.Set(l)
	return nil
}

// Setup installs the default slog logger (which also receives the standard log
// package's output) and returns a function that closes the log file.
func Setup(o Options) (closeFn func() error, err error) {
	if err := SetLevel(o.Level); err != nil {
		return nil, err
	}
	var writers []io.Writer
	closeFn = func() error { return nil }
	if o.File != "" {
		if err := os.MkdirAll(filepath.Dir(o.File), 0o755); err != nil {
			return nil, err
		}
		rotate(o.File)
		f, err := os.OpenFile(o.File, os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0o644)
		if err != nil {
			return nil, err
		}
		writers = append(writers, f)
		closeFn = f.Close
	}
	if o.Stderr || len(writers) == 0 {
		writers = append(writers, os.Stderr)
	}
	handler := slog.NewTextHandler(io.MultiWriter(writers...), &slog.HandlerOptions{Level: level})
	slog.SetDefault(slog.New(handler))
	// Messages from the standard log package (e.g. from libraries) arrive as info.
	log.SetFlags(0)
	return closeFn, nil
}

// rotate keeps one previous log when the file has grown past MaxSize.
func rotate(path string) {
	if fi, err := os.Stat(path); err == nil && fi.Size() > MaxSize {
		_ = os.Rename(path, path+".1")
	}
}
