// Package datadir lays out the app's data folder: one subfolder per game, so each
// game keeps its own collection.json, price-history.json and caches.
//
//	<data>/mtgmanager.lock
//	<data>/mtg/collection.json
//	<data>/mtg/price-history.json
//	<data>/mtg/scryfall-sets.json
package datadir

import (
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"runtime"
)

// CollectionFile is the name of each game's collection file.
const CollectionFile = "collection.json"

// GameDir returns (creating if needed) the folder for a game's files.
func GameDir(root, gameID string) (string, error) {
	dir := filepath.Join(root, gameID)
	return dir, os.MkdirAll(dir, 0o755)
}

// legacyFiles were kept directly in the data folder before multi-game support,
// when Magic was the only game.
var legacyFiles = []string{"collection.json", "price-history.json", "scryfall-sets.json"}

// MoveLegacyFiles moves files from before multi-game support out of the top of the
// data folder into gameDir (Magic's folder). Nothing is overwritten: if gameDir
// already has a collection, the legacy files are left where they are. It returns
// the files moved.
func MoveLegacyFiles(root, gameDir string) ([]string, error) {
	if _, err := os.Stat(filepath.Join(root, CollectionFile)); errors.Is(err, fs.ErrNotExist) {
		return nil, nil // nothing to move
	}
	if _, err := os.Stat(filepath.Join(gameDir, CollectionFile)); err == nil {
		return nil, nil // already migrated; never overwrite
	}
	var moved []string
	for _, name := range legacyFiles {
		src, dst := filepath.Join(root, name), filepath.Join(gameDir, name)
		if _, err := os.Stat(src); errors.Is(err, fs.ErrNotExist) {
			continue
		}
		if _, err := os.Stat(dst); err == nil {
			continue // don't overwrite (e.g. a cache already written there)
		}
		if err := os.Rename(src, dst); err != nil {
			return moved, fmt.Errorf("moving %s to %s: %w", src, dst, err)
		}
		moved = append(moved, name)
	}
	return moved, nil
}

// DefaultRoot is the desktop data folder for appID: the same folder Fyne uses for the
// app's storage, so the desktop window and --web mode share one collection.
//
//	macOS    ~/Library/Preferences/fyne/<app id>
//	Windows  %USERPROFILE%\AppData\Roaming\fyne\<app id>
//	Linux    $XDG_CONFIG_HOME/fyne/<app id> (usually ~/.config/fyne/<app id>)
func DefaultRoot(appID string) (string, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}
	var base string
	switch runtime.GOOS {
	case "darwin":
		base = filepath.Join(home, "Library", "Preferences", "fyne")
	case "windows":
		base = filepath.Join(home, "AppData", "Roaming", "fyne")
	default:
		cfg, err := os.UserConfigDir()
		if err != nil {
			return "", err
		}
		base = filepath.Join(cfg, "fyne")
	}
	return filepath.Join(base, appID), nil
}
