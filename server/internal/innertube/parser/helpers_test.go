package parser_test

import (
	"encoding/json"
	"testing"

	"github.com/iceice666/sunflower/server/internal/innertube/parser"
)

// TestParseSongItem_RunsTitle exercises the firstRunText path that would return
// "" if getString were used with a numeric "0" map key.
func TestParseSongItem_RunsTitle(t *testing.T) {
	raw := []byte(`{
		"videoId": "abc123",
		"title": {
			"runs": [{"text": "Never Gonna Give You Up"}]
		},
		"subtitle": {"runs": []},
		"thumbnail": {"thumbnails": []}
	}`)

	var m map[string]any
	if err := json.Unmarshal(raw, &m); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}

	// Wrap in a minimal next response and parse through ParseNextPage.
	// Direct access requires exported parseSongItem, so test via ParseNextPage
	// with a constructed playlistPanelVideoRenderer.
	nextRaw := []byte(`{
		"contents": {
			"singleColumnMusicWatchNextResultsRenderer": {
				"tabbedRenderer": {
					"watchNextTabbedResultsRenderer": {
						"tabs": [{
							"tabRenderer": {
								"content": {
									"musicQueueRenderer": {
										"content": {
											"playlistPanelRenderer": {
												"contents": [{
													"playlistPanelVideoRenderer": {
														"videoId": "abc123",
														"title": {
															"runs": [{"text": "Never Gonna Give You Up"}]
														},
														"subtitle": {"runs": []},
														"thumbnail": {"thumbnails": []}
													}
												}]
											}
										}
									}
								}
							}
						}]
					}
				}
			}
		}
	}`)

	page := parser.ParseNextPage(nextRaw)
	if len(page.Related) == 0 {
		t.Fatal("expected at least one related item")
	}
	got := page.Related[0]
	if got.VideoID != "abc123" {
		t.Errorf("VideoID: got %q, want %q", got.VideoID, "abc123")
	}
	if got.Title != "Never Gonna Give You Up" {
		t.Errorf("Title: got %q, want %q — firstRunText may be broken", got.Title, "Never Gonna Give You Up")
	}
}

// TestExtractContinuation_ArrayNav exercises the fixed extractContinuation that
// uses getArray + index checks instead of getString with "0" map key.
func TestExtractContinuation_ArrayNav(t *testing.T) {
	nextRaw := []byte(`{
		"contents": {
			"singleColumnMusicWatchNextResultsRenderer": {
				"tabbedRenderer": {
					"watchNextTabbedResultsRenderer": {
						"tabs": [{
							"tabRenderer": {
								"content": {
									"musicQueueRenderer": {
										"content": {
											"playlistPanelRenderer": {
												"continuations": [{
													"nextRadioContinuationData": {
														"continuation": "TOKEN_ABC"
													}
												}]
											}
										}
									}
								}
							}
						}]
					}
				}
			}
		}
	}`)

	page := parser.ParseNextPage(nextRaw)
	if page.Continuation.IsZero() {
		t.Error("expected non-zero continuation token")
	}
	if string(page.Continuation) != "TOKEN_ABC" {
		t.Errorf("continuation: got %q, want %q", string(page.Continuation), "TOKEN_ABC")
	}
}

// TestExtractContinuation_Fallback exercises the fallback path via
// continuationContents.
func TestExtractContinuation_Fallback(t *testing.T) {
	nextRaw := []byte(`{
		"continuationContents": {
			"playlistPanelContinuation": {
				"continuations": [{
					"nextRadioContinuationData": {
						"continuation": "TOKEN_FALLBACK"
					}
				}]
			}
		}
	}`)

	page := parser.ParseNextPage(nextRaw)
	if page.Continuation.IsZero() {
		t.Error("expected non-zero continuation from fallback path")
	}
	if string(page.Continuation) != "TOKEN_FALLBACK" {
		t.Errorf("continuation: got %q, want %q", string(page.Continuation), "TOKEN_FALLBACK")
	}
}
