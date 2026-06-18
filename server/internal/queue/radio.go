// Package queue builds and stores server-side playback queues. A queue is
// seeded (from a song, album, playlist, artist radio, or the user's liked
// songs) and expanded into a materialized item list that the client consumes
// via GET /api/v1/next.
package queue

import (
	"context"
	"encoding/json"

	"github.com/iceice666/sunflower/server/internal/innertube/continuation"
	"github.com/iceice666/sunflower/server/internal/innertube/models"
	"github.com/iceice666/sunflower/server/internal/innertube/parser"
)

// Item is one materialized queue entry. Metadata is carried verbatim so the
// /next handler can surface title/artist without a second lookup.
type Item struct {
	MediaID    string   `json:"media_id"`
	Title      string   `json:"title"`
	Artists    []string `json:"artists"`
	DurationMs int      `json:"duration_ms"`
}

// NextClient is the InnerTube surface the radio expander needs. *innertube.Client
// satisfies it; tests substitute a fake.
type NextClient interface {
	Next(ctx context.Context, videoID string, cont continuation.Cursor) (json.RawMessage, error)
}

// itemFromSong converts an InnerTube SongItem into a queue Item, prefixing the
// video id into media_id form ("yt:<id>").
func itemFromSong(s models.SongItem) Item {
	return Item{
		MediaID:    "yt:" + s.VideoID,
		Title:      s.Title,
		Artists:    s.Artists,
		DurationMs: s.DurationMs,
	}
}

// ExpandRadio builds a radio queue from a YouTube song seed by calling
// /next and following continuations until at least minItems are collected or
// the continuation chain ends. The seed song is always the first item.
func ExpandRadio(ctx context.Context, c NextClient, seedVideoID string, minItems int) ([]Item, continuation.Cursor, error) {
	raw, err := c.Next(ctx, seedVideoID, nil)
	if err != nil {
		return nil, nil, err
	}
	page := parser.ParseNextPage(raw)

	items := make([]Item, 0, minItems)
	seen := map[string]bool{}

	add := func(songs []models.SongItem) {
		for _, s := range songs {
			if s.VideoID == "" || seen[s.VideoID] {
				continue
			}
			seen[s.VideoID] = true
			items = append(items, itemFromSong(s))
		}
	}
	add(page.Related)

	cont := page.Continuation
	// Follow continuations until we have enough items or run out. Bounded by a
	// hard iteration cap as well as a progress check so a misbehaving upstream
	// (always returns a cursor, never new items) can never spin forever.
	const maxPages = 10
	for pages := 0; len(items) < minItems && !cont.IsZero() && pages < maxPages; pages++ {
		raw, err := c.Next(ctx, seedVideoID, cont)
		if err != nil {
			break // partial queue is still usable; surface what we have
		}
		nextPage := parser.ParseNextPage(raw)
		before := len(items)
		add(nextPage.Related)
		cont = nextPage.Continuation
		if len(items) == before {
			break // no progress — stop to avoid a spin loop
		}
	}

	return items, cont, nil
}
