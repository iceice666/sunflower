package queue

import (
	"context"
	"encoding/json"
	"testing"

	"github.com/iceice666/sunflower/server/internal/innertube/continuation"
)

// fakeNext returns canned /next pages. Each call pops the next scripted page;
// the cursor it embeds drives the expander's continuation loop.
type fakeNext struct {
	pages []string
	calls int
}

func (f *fakeNext) Next(_ context.Context, _ string, _ continuation.Cursor) (json.RawMessage, error) {
	if f.calls >= len(f.pages) {
		return json.RawMessage(`{}`), nil
	}
	p := f.pages[f.calls]
	f.calls++
	return json.RawMessage(p), nil
}

// nextPageJSON builds a minimal /next response with the given related video IDs
// and an optional continuation token.
func nextPageJSON(videoIDs []string, cont string) string {
	items := ""
	for i, id := range videoIDs {
		if i > 0 {
			items += ","
		}
		items += `{"playlistPanelVideoRenderer":{"videoId":"` + id + `",` +
			`"title":{"runs":[{"text":"Song ` + id + `"}]}}}`
	}
	conts := ""
	if cont != "" {
		conts = `,"continuations":[{"nextRadioContinuationData":{"continuation":"` + cont + `"}}]`
	}
	return `{"contents":{"singleColumnMusicWatchNextResultsRenderer":{"tabbedRenderer":` +
		`{"watchNextTabbedResultsRenderer":{"tabs":[{"tabRenderer":{"content":` +
		`{"musicQueueRenderer":{"content":{"playlistPanelRenderer":{"contents":[` +
		items + `]` + conts + `}}}}}}]}}}}}`
}

func TestExpandRadioCollectsAcrossContinuations(t *testing.T) {
	f := &fakeNext{pages: []string{
		nextPageJSON([]string{"a", "b", "c"}, "cont1"),
		nextPageJSON([]string{"d", "e", "f"}, "cont2"),
		nextPageJSON([]string{"g", "h", "i", "j", "k"}, ""),
	}}

	items, _, err := ExpandRadio(context.Background(), f, "a", 10)
	if err != nil {
		t.Fatalf("ExpandRadio: %v", err)
	}
	if len(items) < 10 {
		t.Fatalf("got %d items, want ≥10 (lookahead buffer floor)", len(items))
	}
	// media_id must be in "yt:<id>" form.
	if items[0].MediaID != "yt:a" {
		t.Fatalf("first item media_id = %q, want yt:a", items[0].MediaID)
	}
}

func TestExpandRadioDeduplicates(t *testing.T) {
	f := &fakeNext{pages: []string{
		nextPageJSON([]string{"a", "b", "a", "b"}, ""),
	}}
	items, _, err := ExpandRadio(context.Background(), f, "a", 10)
	if err != nil {
		t.Fatalf("ExpandRadio: %v", err)
	}
	if len(items) != 2 {
		t.Fatalf("got %d items, want 2 after dedup", len(items))
	}
}

func TestExpandRadioStopsWhenNoProgress(t *testing.T) {
	// A page that always advertises a continuation but never adds new items must
	// not spin forever: dedup makes len() stall, so the loop must break.
	f := &fakeNext{pages: []string{
		nextPageJSON([]string{"a", "b"}, "cont"),
		nextPageJSON([]string{"a", "b"}, "cont"), // same ids → no progress
	}}
	items, _, err := ExpandRadio(context.Background(), f, "a", 10)
	if err != nil {
		t.Fatalf("ExpandRadio: %v", err)
	}
	if len(items) != 2 {
		t.Fatalf("got %d items, want 2", len(items))
	}
}
