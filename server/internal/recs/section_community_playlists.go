package recs

import (
	"context"

	"github.com/google/uuid"
	"github.com/iceice666/sunflower/server/internal/innertube/parser"
)

const communityPlaylistLimit = 15

// CommunityPlaylists surfaces community/editorial content via a generic search.
// In v1 this is a lightweight "popular music" search; richer playlist-renderer
// parsing is future work. Degrades to empty when YT is unavailable.
func (e *Engine) CommunityPlaylists(ctx context.Context, userID uuid.UUID, prefs Prefs) Section {
	sec := Section{ID: "community_playlists", Title: "Community Playlists", Kind: "community_playlists"}
	if e.yt == nil {
		return sec
	}
	callCtx, cancel := contextWithTimeout(ctx, e.callTimeout)
	defer cancel()
	raw, err := e.yt.Search(callCtx, "popular music playlists")
	if err != nil {
		return sec
	}
	page := parser.ParseSearchPage(raw)
	cands := make([]Candidate, 0, len(page.Songs))
	for _, s := range page.Songs {
		if c := songItemToCandidate(s, 0.4); c.MediaID != "" {
			cands = append(cands, c)
		}
	}
	cands = applyFilters(cands, prefFilters(prefs)...)
	impr := e.loadImpressions(ctx, userID)
	sec.Items = rankAndDiversify(cands, impr, e.clock(), communityPlaylistLimit)
	return sec
}
