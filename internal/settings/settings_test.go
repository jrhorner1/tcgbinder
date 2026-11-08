package settings

import (
	"os"
	"path/filepath"
	"testing"
)

func TestOpenDefaultsAndPartialFiles(t *testing.T) {
	dir := t.TempDir()
	if s := Open(dir).Get(); s != Defaults() {
		t.Fatalf("no file: %+v", s)
	}
	// Missing fields keep their defaults; invalid values are replaced.
	os.WriteFile(filepath.Join(dir, FileName), []byte(`{"theme":"dark","card_order":"sideways","show_images":false}`), 0o644)
	s := Open(dir).Get()
	if s.Theme != "dark" || s.CardOrder != "name" || s.ShowImages || !s.ConfirmDelete || s.PriceRefresh != "daily" {
		t.Fatalf("partial file: %+v", s)
	}
}

func TestDamagedFileIsSetAside(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, FileName)
	os.WriteFile(path, []byte(`{not json`), 0o644)
	if s := Open(dir).Get(); s != Defaults() {
		t.Fatalf("damaged file: %+v", s)
	}
	if _, err := os.Stat(path + ".unreadable"); err != nil {
		t.Fatalf("damaged file not kept aside: %v", err)
	}
}

func TestSetSavesValidatesAndNotifies(t *testing.T) {
	dir := t.TempDir()
	st := Open(dir)
	var changes int
	st.OnChange(func(old, new Settings) {
		changes++
		if old.Theme != "system" || new.Theme != "light" {
			t.Errorf("change %q -> %q", old.Theme, new.Theme)
		}
	})
	if err := st.Update(func(s *Settings) { s.Theme = "light" }); err != nil {
		t.Fatal(err)
	}
	if err := st.Update(func(s *Settings) { s.Theme = "light" }); err != nil || changes != 1 {
		t.Fatalf("an unchanged save should not notify: %v, %d changes", err, changes)
	}
	if err := st.Update(func(s *Settings) { s.TextSize = "huge" }); err == nil {
		t.Fatal("invalid text size accepted")
	}
	if s := Open(dir).Get(); s.Theme != "light" || s.TextSize != "normal" {
		t.Fatalf("reopened: %+v", s)
	}
	if Defaults().TextScale() != 1 || (Settings{TextSize: "larger"}).TextScale() != 1.3 {
		t.Fatal("text scale")
	}
}
