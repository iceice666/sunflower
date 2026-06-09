package library

import (
	"bytes"
	"fmt"
	"os"
	"path/filepath"

	"github.com/disintegration/imaging"
)

// SaveCoverArt decodes pic bytes and saves resized JPEG versions under
// <dataDir>/art/<mediaID>/{256,512,1024}.jpg. No-op if pic is empty.
func SaveCoverArt(pic []byte, mediaID, dataDir string) error {
	if len(pic) == 0 {
		return nil
	}

	img, err := imaging.Decode(bytes.NewReader(pic), imaging.AutoOrientation(true))
	if err != nil {
		return fmt.Errorf("decode cover art: %w", err)
	}

	dir := filepath.Join(dataDir, "art", mediaID)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return err
	}

	for _, size := range []int{256, 512, 1024} {
		resized := imaging.Fit(img, size, size, imaging.Lanczos)
		dest := filepath.Join(dir, fmt.Sprintf("%d.jpg", size))
		if err := imaging.Save(resized, dest); err != nil {
			return fmt.Errorf("save %dpx: %w", size, err)
		}
	}
	return nil
}
