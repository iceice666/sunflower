// server/internal/innertube/parser/artist_page.go
package parser

import (
	"encoding/json"

	"github.com/iceice666/sunflower/server/internal/innertube/models"
)

// ParseArtistPage parses an artist browse response.
func ParseArtistPage(raw json.RawMessage) models.ArtistItem {
	m := unmarshalMap(raw)
	if m == nil {
		return models.ArtistItem{}
	}
	// Use firstRunText to avoid "0" string-key bug in runs array.
	return models.ArtistItem{
		Name: firstRunText(getMap(m, "header", "musicImmersiveHeaderRenderer"), "title"),
	}
}
