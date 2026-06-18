// server/internal/innertube/parser/related_page.go
package parser

import (
	"encoding/json"

	"github.com/iceice666/sunflower/server/internal/innertube/models"
)

// ParseRelatedPage parses related items from a browse response.
func ParseRelatedPage(raw json.RawMessage) []models.SongItem {
	m := unmarshalMap(raw)
	if m == nil {
		return nil
	}

	var items []models.SongItem

	// Traverse tabs using arrayFirstMap to avoid "0" string-key bug.
	for _, item := range getArray(m, "contents", "singleColumnBrowseResultsRenderer", "tabs") {
		tab, ok := item.(map[string]any)
		if !ok {
			continue
		}
		tabContent := getMap(tab, "tabRenderer", "content", "sectionListRenderer")
		for _, sec := range getArray(tabContent, "contents") {
			s, ok := sec.(map[string]any)
			if !ok {
				continue
			}
			shelf := getMap(s, "musicShelfRenderer")
			if shelf == nil {
				continue
			}
			for _, r := range getArray(shelf, "contents") {
				ri, ok := r.(map[string]any)
				if !ok {
					continue
				}
				if mr := getMap(ri, "musicResponsiveListItemRenderer"); mr != nil {
					items = append(items, parseSongItem(mr))
				}
				// Unknown renderer — skip silently.
			}
		}
	}

	return items
}
