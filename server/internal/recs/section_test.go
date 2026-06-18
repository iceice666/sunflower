package recs

import (
	"context"
	"encoding/json"
	"testing"
	"time"

	"github.com/google/uuid"
	"github.com/iceice666/sunflower/server/internal/innertube/continuation"
	"github.com/iceice666/sunflower/server/internal/innertube/models"
	"github.com/rs/zerolog"
)

// fakeYT is a deterministic InnerTube stand-in. nextByVideo maps a seed video id
// to the related-songs JSON returned by /next; browseJSON / searchJSON are fixed
// payloads for browse/search.
type fakeYT struct {
	nextByVideo map[string]json.RawMessage
	browseJSON  json.RawMessage
	searchJSON  json.RawMessage
	calls       int
}

func (f *fakeYT) Next(_ context.Context, videoID string, _ continuation.Cursor) (json.RawMessage, error) {
	f.calls++
	if r, ok := f.nextByVideo[videoID]; ok {
		return r, nil
	}
	return json.RawMessage(`{}`), nil
}

func (f *fakeYT) Browse(_ context.Context, _ string, _ continuation.Cursor) (json.RawMessage, error) {
	f.calls++
	if f.browseJSON == nil {
		return json.RawMessage(`{}`), nil
	}
	return f.browseJSON, nil
}

func (f *fakeYT) Search(_ context.Context, _ string) (json.RawMessage, error) {
	f.calls++
	if f.searchJSON == nil {
		return json.RawMessage(`{}`), nil
	}
	return f.searchJSON, nil
}

func (f *fakeYT) Player(_ context.Context, _ string) (models.PlayerResponse, error) {
	return models.PlayerResponse{}, nil
}

// nextPageJSON builds a minimal /next response whose related shelf contains the
// given video ids, matching the path parser.extractRelatedItems walks.
func nextPageJSON(videoIDs ...string) json.RawMessage {
	var contents []any
	for _, id := range videoIDs {
		contents = append(contents, map[string]any{
			"playlistPanelVideoRenderer": map[string]any{
				"videoId": id,
				"title":   map[string]any{"runs": []any{map[string]any{"text": "Song " + id}}},
			},
		})
	}
	doc := map[string]any{
		"contents": map[string]any{
			"singleColumnMusicWatchNextResultsRenderer": map[string]any{
				"tabbedRenderer": map[string]any{
					"watchNextTabbedResultsRenderer": map[string]any{
						"tabs": []any{
							map[string]any{
								"tabRenderer": map[string]any{
									"content": map[string]any{
										"musicQueueRenderer": map[string]any{
											"content": map[string]any{
												"playlistPanelRenderer": map[string]any{
													"contents": contents,
												},
											},
										},
									},
								},
							},
						},
					},
				},
			},
		},
	}
	b, _ := json.Marshal(doc)
	return b
}

func testEngine(yt YTClient) *Engine {
	return &Engine{
		yt:          yt,
		log:         zerolog.Nop(),
		clock:       func() time.Time { return time.Unix(1_700_000_000, 0) },
		maxFanout:   5,
		callTimeout: 2 * time.Second,
	}
}

func TestFanOutRelated_MergesAndDedups(t *testing.T) {
	yt := &fakeYT{nextByVideo: map[string]json.RawMessage{
		"v1": nextPageJSON("a", "b"),
		"v2": nextPageJSON("b", "c"), // b duplicates v1's b
	}}
	e := testEngine(yt)
	got := e.fanOutRelated(context.Background(), []string{"yt:v1", "yt:v2"})

	ids := map[string]bool{}
	for _, c := range got {
		if ids[c.MediaID] {
			t.Fatalf("duplicate media id in merged output: %s", c.MediaID)
		}
		ids[c.MediaID] = true
	}
	for _, want := range []string{"yt:a", "yt:b", "yt:c"} {
		if !ids[want] {
			t.Errorf("missing expected related candidate %s; got %v", want, ids)
		}
	}
}

func TestFanOutRelated_DropsFailedSeedsButKeepsRest(t *testing.T) {
	yt := &fakeYT{nextByVideo: map[string]json.RawMessage{
		"v1": nextPageJSON("a"),
		// v2 absent → fake returns empty {} → no related items, no error
	}}
	e := testEngine(yt)
	got := e.fanOutRelated(context.Background(), []string{"yt:v1", "yt:v2"})
	if len(got) != 1 || got[0].MediaID != "yt:a" {
		t.Fatalf("want only yt:a from the good seed, got %+v", got)
	}
}

func TestYouTubeHome_NilYTDegradesEmpty(t *testing.T) {
	e := testEngine(nil)
	e.yt = nil
	res := e.YouTubeHome(context.Background(), uuid.Nil, Prefs{})
	if len(res.Items) != 0 {
		t.Fatalf("nil YT should yield empty home section, got %d items", len(res.Items))
	}
}

func mustUUID(s string) uuid.UUID {
	return uuid.MustParse(s)
}
