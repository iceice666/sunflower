package recs

import (
	"context"

	"github.com/google/uuid"
)

// quickPicksLimit caps the Quick Picks row (m5: capped at 20).
const quickPicksLimit = 20

// QuickPicks builds the local-first Quick Picks section: most-played plus a few
// forgotten favorites, ranked and diversified. No network in the critical path —
// it must render within 500 ms of the response (m5 acceptance), so it draws only
// from local play history.
func (e *Engine) QuickPicks(ctx context.Context, userID uuid.UUID, prefs Prefs) Section {
	cands := e.mostPlayedCandidates(ctx, userID, quickPicksLimit*2)
	cands = append(cands, e.forgottenCandidates(ctx, userID, quickPicksLimit)...)

	cands = applyFilters(cands, prefFilters(prefs)...)
	impr := e.loadImpressions(ctx, userID)
	items := rankAndDiversify(cands, impr, e.clock(), quickPicksLimit)

	return Section{
		ID:    "quick_picks",
		Title: "Quick Picks",
		Kind:  "quick_picks",
		Items: items,
	}
}
