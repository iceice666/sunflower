package recs

import (
	"context"

	"github.com/google/uuid"
	"github.com/iceice666/sunflower/server/internal/innertube/models"
	"github.com/iceice666/sunflower/server/internal/innertube/parser"
)

const ytHomeLimit = 30

// ytHomeBrowseID is the InnerTube browse id for the music home feed.
const ytHomeBrowseID = "FEmusic_home"

// ytHomeResult carries the section plus the chip labels (mood/genre filters)
// extracted from the home response, which the engine surfaces at the top level.
type ytHomeResult struct {
	Section
	chips []string
}

// YouTubeHome builds the personalized YouTube Music home row plus its chips
// (Relax, Workout, Sleep, …). Degrades to an empty result when YT is
// unavailable or guest-mode returns nothing.
func (e *Engine) YouTubeHome(ctx context.Context, userID uuid.UUID, prefs Prefs) ytHomeResult {
	res := ytHomeResult{Section: Section{ID: "yt_home", Title: "From YouTube Music", Kind: "yt_home"}}
	if e.yt == nil {
		return res
	}
	callCtx, cancel := contextWithTimeout(ctx, e.callTimeout)
	defer cancel()
	raw, err := e.yt.Browse(callCtx, ytHomeBrowseID, nil)
	if err != nil {
		return res
	}
	page := parser.ParseHomePage(raw)
	res.chips = page.Chips

	// Flatten the home sections' song items into one candidate pool.
	var cands []Candidate
	for _, section := range page.Sections {
		for _, item := range section.Items {
			s, ok := item.(models.SongItem)
			if !ok {
				continue // album/artist/playlist tiles aren't directly playable here
			}
			if c := songItemToCandidate(s, 0.5); c.MediaID != "" {
				cands = append(cands, c)
			}
		}
	}
	cands = applyFilters(cands, prefFilters(prefs)...)
	impr := e.loadImpressions(ctx, userID)
	res.Items = rankAndDiversify(cands, impr, e.clock(), ytHomeLimit)
	return res
}
