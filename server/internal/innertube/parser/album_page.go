// server/internal/innertube/parser/album_page.go
package parser

import (
	"encoding/json"

	"github.com/iceice666/sunflower/server/internal/innertube/models"
)

// ParseAlbumPage parses an album browse response.
func ParseAlbumPage(raw json.RawMessage) models.AlbumItem {
	m := unmarshalMap(raw)
	if m == nil {
		return models.AlbumItem{}
	}
	return models.AlbumItem{
		// Use firstRunText to avoid "0" string-key bug in runs array.
		Title: firstRunText(getMap(m, "header", "musicDetailHeaderRenderer"), "title"),
		// Year: subtitle.simpleText is more stable than subtitle.runs[4].text
		// (index-based path fails silently and is unreliable across YT response variants).
		Year: getString(m, "header", "musicDetailHeaderRenderer", "subtitle", "simpleText"),
	}
}
