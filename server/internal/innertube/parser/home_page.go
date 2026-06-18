// server/internal/innertube/parser/home_page.go
package parser

import (
	"encoding/json"

	"github.com/iceice666/sunflower/server/internal/innertube/models"
)

// ParseHomePage parses the raw /youtubei/v1/browse?browseId=FEmusic_home response.
func ParseHomePage(raw json.RawMessage) models.HomePage {
	m := unmarshalMap(raw)
	if m == nil {
		return models.HomePage{}
	}

	var page models.HomePage

	// Chips (mood/genre filters).
	for _, chip := range getArray(m, "header", "musicImmersiveHeaderRenderer",
		"menu", "chipCloudRenderer", "chips") {
		c, ok := chip.(map[string]any)
		if !ok {
			continue
		}
		// Use firstRunText to extract text from runs[0], not "0" string key.
		text := firstRunText(getMap(c, "chipCloudChipRenderer"), "text")
		if text != "" {
			page.Chips = append(page.Chips, text)
		}
	}

	// Sections — try direct sectionListRenderer path first (non-tabbed),
	// fall back to tabs[0] path when the response is tab-wrapped.
	var sections []any
	sections = getArray(m, "contents", "singleColumnBrowseResultsRenderer",
		"tabbedRenderer", "tabRenderer", "content", "sectionListRenderer", "contents")
	if sections == nil {
		tabs := getArray(m, "contents", "singleColumnBrowseResultsRenderer", "tabs")
		if tab := arrayFirstMap(tabs); tab != nil {
			sections = getArray(tab, "tabRenderer", "content", "sectionListRenderer", "contents")
		}
	}

	for _, s := range sections {
		sec, ok := s.(map[string]any)
		if !ok {
			continue
		}
		section := parseHomeSection(sec)
		if len(section.Items) > 0 || section.Title != "" {
			page.Sections = append(page.Sections, section)
		}
	}

	return page
}

func parseHomeSection(m map[string]any) models.HomeSection {
	var sec models.HomeSection

	if mr := getMap(m, "musicCarouselShelfRenderer"); mr != nil {
		// Use firstRunText to avoid "0" string-key bug in runs array.
		sec.Title = firstRunText(getMap(mr, "header", "musicCarouselShelfBasicHeaderRenderer"), "title")
		for _, item := range getArray(mr, "contents") {
			r, ok := item.(map[string]any)
			if !ok {
				continue
			}
			if mi := getMap(r, "musicTwoRowItemRenderer"); mi != nil {
				// Inspect page type to determine item kind.
				pageType := getString(mi, "navigationEndpoint", "browseEndpoint",
					"browseEndpointContextSupportedConfigs",
					"browseEndpointContextMusicConfig", "pageType")
				switch pageType {
				case "MUSIC_PAGE_TYPE_ALBUM", "MUSIC_PAGE_TYPE_PLAYLIST":
					sec.Items = append(sec.Items, parseAlbumItem(mi))
				case "MUSIC_PAGE_TYPE_ARTIST":
					sec.Items = append(sec.Items, parseArtistItem(mi))
				default:
					// Treat as song if it has a videoId.
					if getString(mi, "navigationEndpoint", "watchEndpoint", "videoId") != "" {
						sec.Items = append(sec.Items, parseSongItem(mi))
					}
				}
			}
			// Unknown renderer — skip silently.
		}
	}

	return sec
}
