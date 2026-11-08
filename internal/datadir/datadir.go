// Package datadir lays out the app's data folder: one subfolder per game, so each
// game keeps its own collection.json, price-history.json and caches.
//
//	<data>/tcgbinder.lock
//	<data>/mtg/collection.json
//	<data>/mtg/price-history.json
//	<data>/mtg/scryfall-sets.json
package datadir

import (
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
