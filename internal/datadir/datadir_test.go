package datadir

import (
	"os"
	"path/filepath"
	"testing"
)

func write(t *testing.T, path, body string) {
	t.Helper()
	if err := os.WriteFile(path, []byte(body), 0o600); err != nil {
		t.Fatal(err)
	}
}

func read(t *testing.T, path string) string {
	t.Helper()
	b, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	return string(b)
}

func TestMoveLegacyFiles(t *testing.T) {
	root := t.TempDir()
	write(t, filepath.Join(root, "collection.json"), "C")
	write(t, filepath.Join(root, "price-history.json"), "P")
	write(t, filepath.Join(root, "scryfall-sets.json"), "S")
	write(t, filepath.Join(root, "mtgmanager.lock"), "123")

	dir, err := GameDir(root, "mtg")
	if err != nil {
		t.Fatal(err)
	}
	moved, err := MoveLegacyFiles(root, dir)
	if err != nil || len(moved) != 3 {
		t.Fatalf("moved %v, %v", moved, err)
	}
	for name, body := range map[string]string{"collection.json": "C", "price-history.json": "P", "scryfall-sets.json": "S"} {
		if got := read(t, filepath.Join(dir, name)); got != body {
			t.Errorf("%s = %q", name, got)
		}
		if _, err := os.Stat(filepath.Join(root, name)); !os.IsNotExist(err) {
			t.Errorf("%s should have moved out of the data folder", name)
		}
	}
	if read(t, filepath.Join(root, "mtgmanager.lock")) != "123" {
		t.Error("the lock file belongs to the app, not a game, and must stay")
	}

	// Running again is a no-op.
	if moved, err := MoveLegacyFiles(root, dir); err != nil || len(moved) != 0 {
		t.Fatalf("second run moved %v, %v", moved, err)
	}
}

func TestMoveLegacyFilesNeverOverwrites(t *testing.T) {
	root := t.TempDir()
	dir, _ := GameDir(root, "mtg")
	write(t, filepath.Join(root, "collection.json"), "old")
	write(t, filepath.Join(dir, "collection.json"), "current")

	moved, err := MoveLegacyFiles(root, dir)
	if err != nil || len(moved) != 0 {
		t.Fatalf("moved %v, %v", moved, err)
	}
	if read(t, filepath.Join(dir, "collection.json")) != "current" || read(t, filepath.Join(root, "collection.json")) != "old" {
		t.Fatal("both files must be left exactly as they were")
	}
}
