// Package imagecache downloads card images and keeps recently viewed ones in memory.
package imagecache

import (
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"sync"
	"time"
)

// httpClient is shared so connections to image hosts are reused.
var httpClient = &http.Client{Timeout: 10 * time.Second}

const maxCachedImages = 200

// imageCache keeps recently viewed card images in memory so reselecting a card is instant.
var imageCache = struct {
	sync.Mutex
	data  map[string][]byte
	order []string
}{data: map[string][]byte{}}

// fetchImage downloads (or returns a cached copy of) the image at url.
// It blocks on the network, so call it from a goroutine, never the UI thread.
func Fetch(url string) ([]byte, error) {
	if url == "" {
		return nil, fmt.Errorf("no image url")
	}

	imageCache.Lock()
	if b, ok := imageCache.data[url]; ok {
		imageCache.Unlock()
		return b, nil
	}
	imageCache.Unlock()

	slog.Debug("image download", "url", url)
	req, err := http.NewRequest("GET", url, nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("User-Agent", "TCGBinder/0.1")
	req.Header.Set("Accept", "*/*")

	resp, err := httpClient.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("image request failed: %s", resp.Status)
	}
	b, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, err
	}
	slog.Debug("image downloaded", "url", url, "bytes", len(b))

	imageCache.Lock()
	if _, ok := imageCache.data[url]; !ok {
		imageCache.data[url] = b
		imageCache.order = append(imageCache.order, url)
		if len(imageCache.order) > maxCachedImages {
			delete(imageCache.data, imageCache.order[0])
			imageCache.order = imageCache.order[1:]
		}
	}
	imageCache.Unlock()
	return b, nil
}
