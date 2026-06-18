package recs

import (
	"context"
	"strings"

	"github.com/google/uuid"
	"github.com/iceice666/sunflower/server/internal/db/gen"
	"github.com/iceice666/sunflower/server/internal/innertube/parser"
	"github.com/jackc/pgx/v5/pgtype"
)

const similarArtistLimit = 20

// SimilarToArtists builds one "Similar to <artist>" section per top-N most-played
// artist. Each row expands the artist's browse page into related songs. Returns
// the sections in artist-rank order; empty sections are kept out by the caller.
func (e *Engine) SimilarToArtists(ctx context.Context, userID uuid.UUID, prefs Prefs, n int) []Section {
	if e.yt == nil || e.db == nil {
		return nil
	}
	rows, err := gen.New(e.db).MostPlayedArtists(ctx, gen.MostPlayedArtistsParams{
		UserID:   pgtype.UUID{Bytes: userID, Valid: true},
		Since:    pgtype.Timestamptz{Time: e.clock().Add(-recentWindow), Valid: true},
		PageSize: int32(n),
	})
	if err != nil || len(rows) == 0 {
		return nil
	}

	impr := e.loadImpressions(ctx, userID)
	var sections []Section
	for _, ar := range rows {
		if !ar.ArtistID.Valid {
			continue
		}
		browseID := strings.TrimPrefix(ar.ArtistID.String, "yt:")
		sec := e.similarToArtist(ctx, browseID, ar.ArtistName, prefs, impr)
		sections = append(sections, sec)
	}
	return sections
}

// similarToArtist expands a single artist browse page into a ranked section.
func (e *Engine) similarToArtist(ctx context.Context, browseID, name string, prefs Prefs, impr map[string]int) Section {
	sec := Section{
		ID:    "similar_artist:" + browseID,
		Title: "Similar to " + name,
		Kind:  "similar_artist",
		Seed:  name,
	}
	callCtx, cancel := contextWithTimeout(ctx, e.callTimeout)
	defer cancel()
	raw, err := e.yt.Browse(callCtx, browseID, nil)
	if err != nil {
		return sec
	}
	related := parser.ParseRelatedPage(raw)
	cands := make([]Candidate, 0, len(related))
	for _, s := range related {
		if c := songItemToCandidate(s, 0.6); c.MediaID != "" {
			cands = append(cands, c)
		}
	}
	cands = applyFilters(cands, prefFilters(prefs)...)
	sec.Items = rankAndDiversify(cands, impr, e.clock(), similarArtistLimit)
	return sec
}
