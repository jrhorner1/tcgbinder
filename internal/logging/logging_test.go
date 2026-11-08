package logging

import (
	"log"
	"log/slog"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestSetupWritesLevelsToFile(t *testing.T) {
	path := filepath.Join(t.TempDir(), "logs", "app.log")
	closeFn, err := Setup(Options{File: path, Level: "warn"})
	if err != nil {
		t.Fatal(err)
	}
	slog.Info("hidden below warn")
	slog.Warn("price refresh failed", "game", "mtg")
	log.Printf("from the standard log package") // bridged at info: filtered out
	closeFn()
	slog.SetDefault(slog.New(slog.NewTextHandler(os.Stderr, nil)))

	b, _ := os.ReadFile(path)
	out := string(b)
	if strings.Contains(out, "hidden") || strings.Contains(out, "standard log") {
		t.Errorf("info messages should be filtered at warn:\n%s", out)
	}
	if !strings.Contains(out, "level=WARN") || !strings.Contains(out, `msg="price refresh failed" game=mtg`) {
		t.Errorf("missing structured warning:\n%s", out)
	}
}

func TestRotate(t *testing.T) {
	path := filepath.Join(t.TempDir(), "app.log")
	os.WriteFile(path, make([]byte, MaxSize+1), 0o644)
	closeFn, err := Setup(Options{File: path, Level: "info"})
	if err != nil {
		t.Fatal(err)
	}
	closeFn()
	slog.SetDefault(slog.New(slog.NewTextHandler(os.Stderr, nil)))
	if fi, err := os.Stat(path + ".1"); err != nil || fi.Size() != MaxSize+1 {
		t.Fatalf("old log should be kept as .1: %v", err)
	}
	if fi, _ := os.Stat(path); fi.Size() != 0 {
		t.Fatalf("new log should start empty, has %d bytes", fi.Size())
	}
}

func TestParseLevel(t *testing.T) {
	if _, err := ParseLevel("loud"); err == nil {
		t.Fatal("unknown levels should be rejected")
	}
	if l, _ := ParseLevel("DEBUG"); l != slog.LevelDebug {
		t.Fatal("levels are case-insensitive")
	}
}
