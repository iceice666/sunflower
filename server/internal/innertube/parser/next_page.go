// server/internal/innertube/parser/next_page.go
package parser

import (
	"encoding/json"

	"github.com/iceice666/sunflower/server/internal/innertube/continuation"
	"github.com/iceice666/sunflower/server/internal/innertube/models"
)

// ParseNextPage parses the raw JSON response from /youtubei/v1/next.
// Missing fields return zero values; unknown renderers are skipped.
func ParseNextPage(raw json.RawMessage) models.NextPage {
	m := unmarshalMap(raw)
	if m == nil {
		return models.NextPage{}
	}

	var page models.NextPage

	// Extract current item from videoDetails.
	// Note: videoDetails is typically present in /player responses, not /next.
	// It may be absent; Current will be zero in that case.
	if vd := getMap(m, "videoDetails"); vd != nil {
		page.Current = models.SongItem{
			VideoID: getString(vd, "videoId"),
			Title:   getString(vd, "title"),
		}
	}

	// Extract related items from the automix/related shelf.
	page.Related = extractRelatedItems(m)

	// Extract continuation token.
	page.Continuation = extractContinuation(m)

	return page
}

func extractRelatedItems(m map[string]any) []models.SongItem {
	tabs := getArray(m, "contents", "singleColumnMusicWatchNextResultsRenderer",
		"tabbedRenderer", "watchNextTabbedResultsRenderer", "tabs")

	var items []models.SongItem
	for _, tab := range tabs {
		t, ok := tab.(map[string]any)
		if !ok {
			continue
		}
		content := getMap(t, "tabRenderer", "content")
		if content == nil {
			continue
		}
		musicQueue := getMap(content, "musicQueueRenderer")
		if musicQueue == nil {
			continue
		}
		for _, item := range getArray(musicQueue, "content", "playlistPanelRenderer", "contents") {
			r, ok := item.(map[string]any)
			if !ok {
				continue
			}
			if ppvr := getMap(r, "playlistPanelVideoRenderer"); ppvr != nil {
				items = append(items, parseSongItem(ppvr))
			}
			// unknown renderer — skip silently
		}
	}
	return items
}

func extractContinuation(m map[string]any) continuation.Cursor {
	// Path via tabs[0] → musicQueueRenderer → playlistPanelRenderer → continuations[0]
	tabs := getArray(m, "contents", "singleColumnMusicWatchNextResultsRenderer",
		"tabbedRenderer", "watchNextTabbedResultsRenderer", "tabs")
	if len(tabs) > 0 {
		if tab, ok := tabs[0].(map[string]any); ok {
			queue := getMap(tab, "tabRenderer", "content", "musicQueueRenderer")
			if queue != nil {
				conts := getArray(queue, "content", "playlistPanelRenderer", "continuations")
				if len(conts) > 0 {
					if c, ok := conts[0].(map[string]any); ok {
						tok := getString(c, "nextRadioContinuationData", "continuation")
						if tok != "" {
							return continuation.Cursor(tok)
						}
					}
				}
			}
		}
	}

	// Fallback: continuation response shape
	conts := getArray(m, "continuationContents", "playlistPanelContinuation", "continuations")
	if len(conts) > 0 {
		if c, ok := conts[0].(map[string]any); ok {
			tok := getString(c, "nextRadioContinuationData", "continuation")
			if tok != "" {
				return continuation.Cursor(tok)
			}
		}
	}
	return nil
}
