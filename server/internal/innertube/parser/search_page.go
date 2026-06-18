// server/internal/innertube/parser/search_page.go
package parser

import (
	"encoding/json"

	"github.com/iceice666/sunflower/server/internal/innertube/continuation"
	"github.com/iceice666/sunflower/server/internal/innertube/models"
)

// ParseSearchPage parses a /youtubei/v1/search response.
func ParseSearchPage(raw json.RawMessage) models.SearchPage {
	m := unmarshalMap(raw)
	if m == nil {
		return models.SearchPage{}
	}

	var page models.SearchPage

	// Build contents from tabs[0] or continuation path.
	// Cannot use string "0" key — use arrayFirstMap to get tabs[0].
	tabs := getArray(m, "contents", "tabbedSearchResultsRenderer", "tabs")
	tab := arrayFirstMap(tabs)
	var contents []any
	if tab != nil {
		contents = getArray(tab, "tabRenderer", "content", "sectionListRenderer", "contents")
	}
	if contents == nil {
		// Continuation response shape.
		contents = getArray(m, "continuationContents", "musicShelfContinuation", "contents")
	}

	for _, sec := range contents {
		s, ok := sec.(map[string]any)
		if !ok {
			continue
		}
		shelf := getMap(s, "musicShelfRenderer")
		if shelf == nil {
			continue
		}
		for _, item := range getArray(shelf, "contents") {
			r, ok := item.(map[string]any)
			if !ok {
				continue
			}
			mr := getMap(r, "musicResponsiveListItemRenderer")
			if mr == nil {
				// Unknown renderer — skip silently.
				continue
			}

			// Detect item type: prefer videoId check, then browseEndpoint pageType.
			videoID := getString(mr, "playlistItemData", "videoId")
			if videoID == "" {
				videoID = getString(mr, "overlay", "musicItemThumbnailOverlayRenderer",
					"content", "musicPlayButtonRenderer", "playNavigationEndpoint",
					"watchEndpoint", "videoId")
			}
			if videoID != "" {
				page.Songs = append(page.Songs, parseResponsiveListSong(mr))
				continue
			}

			// Check browseEndpoint for album/artist.
			browseID := getString(mr, "navigationEndpoint", "browseEndpoint", "browseId")
			if browseID != "" {
				pageType := getString(mr, "navigationEndpoint", "browseEndpoint",
					"browseEndpointContextSupportedConfigs",
					"browseEndpointContextMusicConfig", "pageType")
				switch pageType {
				case "MUSIC_PAGE_TYPE_ALBUM":
					page.Albums = append(page.Albums, parseAlbumItem(mr))
				case "MUSIC_PAGE_TYPE_ARTIST":
					page.Artists = append(page.Artists, parseArtistItem(mr))
				default:
					page.Songs = append(page.Songs, parseResponsiveListSong(mr))
				}
				continue
			}

			// Fallthrough: treat as song.
			page.Songs = append(page.Songs, parseResponsiveListSong(mr))
		}
	}

	// Continuation token — use arrayFirstMap, not "0" string key.
	conts := getArray(m, "continuationContents", "musicShelfContinuation", "continuations")
	if cont := arrayFirstMap(conts); cont != nil {
		if tok := getString(cont, "nextContinuationData", "continuation"); tok != "" {
			page.Continuation = continuation.Cursor(tok)
		}
	}

	return page
}
