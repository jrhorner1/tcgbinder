package ui

import (
	"fmt"
	"io"
	"os"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/storage"
	"fyne.io/fyne/v2/widget"
)

// startImport imports into the active game: a CSV export from another collection
// app, or a collection .json file written by this app (a backup or another device's).
func (ui *App) startImport() {
	window := ui.window
	{
		fileDialog := dialog.NewFileOpen(func(reader fyne.URIReadCloser, err error) {
			if err != nil {
				dialog.ShowError(fmt.Errorf("file access error: %w", err), window)
				return
			}
			if reader == nil {
				return // cancelled
			}
			// Read through the URI (not a filesystem path) so this also works on mobile/web.
			data, err := io.ReadAll(reader)
			reader.Close()
			if err != nil {
				dialog.ShowError(fmt.Errorf("could not read file: %w", err), window)
				return
			}
			ui.runImport(data, window)
		}, window)
		fileDialog.SetFilter(storage.NewExtensionFileFilter([]string{".csv", ".CSV", ".json", ".JSON"}))
		fileDialog.Resize(fyne.NewSize(700, 500))
		fileDialog.Show()
	}
}

// exportCollection saves the active game's collection file where the user chooses;
// Import accepts it (e.g. as a backup or to move to another device).
func (ui *App) exportCollection() {
	gs := ui.g
	data, err := os.ReadFile(gs.store.FilePath())
	if err != nil {
		dialog.ShowError(fmt.Errorf("could not read the collection: %w", err), ui.window)
		return
	}
	save := dialog.NewFileSave(func(w fyne.URIWriteCloser, err error) {
		if err != nil {
			dialog.ShowError(err, ui.window)
			return
		}
		if w == nil {
			return // cancelled
		}
		defer w.Close()
		if _, err := w.Write(data); err != nil {
			dialog.ShowError(fmt.Errorf("export failed: %w", err), ui.window)
			return
		}
		ui.statusLabel.SetText("Exported " + gs.game.Name() + " collection to " + w.URI().Name())
	}, ui.window)
	save.SetFileName(fmt.Sprintf("%s-collection-%s.json", gs.game.ID(), time.Now().Format("2006-01-02")))
	save.Resize(fyne.NewSize(700, 500))
	save.Show()
}

func (ui *App) runImport(data []byte, window fyne.Window) {
	gs := ui.g
	bar := widget.NewProgressBar()
	status := widget.NewLabel("Reading file…")
	progress := dialog.NewCustomWithoutButtons("Importing into "+gs.game.Name(),
		container.NewVBox(status, bar), window)
	progress.Resize(fyne.NewSize(420, 140))
	progress.Show()

	go func() {
		res, err := gs.svc.Import(data, func(done, total int) {
			runOnUI(func() {
				status.SetText(fmt.Sprintf("Matching cards… %d of %d rows", done, total))
				if total > 0 {
					bar.SetValue(float64(done) / float64(total))
				}
			})
		})
		runOnUI(func() {
			progress.Hide()
			if err != nil {
				dialog.ShowError(fmt.Errorf("import failed: %w", err), window)
				return
			}
			ui.RefreshDataIndex()
			ui.statusLabel.SetText(fmt.Sprintf("Imported %d cards from %s", res.Cards, res.Report.Format))

			text := widget.NewLabel(res.Summary())
			text.Wrapping = fyne.TextWrapWord
			scroll := container.NewVScroll(text)
			minSize := fyne.NewSize(560, 320)
			if isMobile() {
				minSize = fyne.NewSize(300, 360)
			}
			scroll.SetMinSize(minSize)
			dialog.ShowCustom("Import Complete", "Done", scroll, window)
		})
	}()
}
