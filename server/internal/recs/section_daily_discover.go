package recs

import (
	"context"
	"strings"

	"github.com/google/uuid"
	"github.com/iceice666/sunflower/server/internal/db/gen"
	"github.com/iceice666/sunflower/server/internal/innertube/parser"
	"github.com/jackc/pgx/v5/pgtype"
	"golang.org/x/sync/errgroup"
)

const (
	dailyDiscoverSeeds = 5
	dailyDiscoverLimit = 25
)

// DailyDiscover expands up to 5 liked songs into related tracks via /next,
// fanning out (bounded) and merging. Seed songs themselves are excluded from the
// result (impression dedupe / "don't recommend what they already like").
func (e *Engine) DailyDiscover(ctx context.Context, userID uuid.UUID, prefs Prefs) Section {
	sec := Section{ID: "daily_discover", Title: "Daily Discover", Kind: "daily_discover"}
	if e.yt == nil || e.db == nil {
		return sec
	}

	seeds := e.likedSeeds(ctx, userID, dailyDiscoverSeeds)
	if len(seeds) == 0 {
		return sec
	}
	seedSet := map[string]bool{}
	for _, s := range seeds {
		seedSet[s] = true
	}

	// Bounded fan-out: one /next per seed, capped at maxFanout concurrent.
	merged := e.fanOutRelated(ctx, seeds)

	// Exclude the seeds themselves.
	cands := make([]Candidate, 0, len(merged))
	for _, c := range merged {
		if seedSet[c.MediaID] {
			continue
		}
		cands = append(cands, c)
	}

	cands = applyFilters(cands, prefFilters(prefs)...)
	impr := e.loadImpressions(ctx, userID)
	sec.Items = rankAndDiversify(cands, impr, e.clock(), dailyDiscoverLimit)
	return sec
}

// likedSeeds returns up to n liked song media_ids that are YouTube tracks
// (videoID-bearing) so /next can expand them.
func (e *Engine) likedSeeds(ctx context.Context, userID uuid.UUID, n int) []string {
	rows, err := gen.New(e.db).ListLikes(ctx, gen.ListLikesParams{
		UserID:   pgtype.UUID{Bytes: userID, Valid: true},
		PageSize: int32(n * 4), // overfetch; filter to yt below
	})
	if err != nil {
		return nil
	}
	var out []string
	for _, r := range rows {
		if strings.HasPrefix(r.SongMediaID, "yt:") {
			out = append(out, r.SongMediaID)
			if len(out) >= n {
				break
			}
		}
	}
	return out
}

// fanOutRelated calls /next for each seed concurrently (bounded by maxFanout)
// and returns the merged, de-duplicated related candidates. Failed seeds are
// dropped silently.
func (e *Engine) fanOutRelated(ctx context.Context, seeds []string) []Candidate {
	results := make([][]Candidate, len(seeds))

	g, gctx := errgroup.WithContext(ctx)
	g.SetLimit(e.maxFanout)
	for i, seed := range seeds {
		i, seed := i, seed
		g.Go(func() error {
			videoID := strings.TrimPrefix(seed, "yt:")
			callCtx, cancel := contextWithTimeout(gctx, e.callTimeout)
			defer cancel()
			raw, err := e.yt.Next(callCtx, videoID, nil)
			if err != nil {
				return nil // drop this seed; never fail the group
			}
			page := parser.ParseNextPage(raw)
			cs := make([]Candidate, 0, len(page.Related))
			for _, s := range page.Related {
				if c := songItemToCandidate(s, 0.7); c.MediaID != "" {
					cs = append(cs, c)
				}
			}
			results[i] = cs
			return nil
		})
	}
	_ = g.Wait() // errors are swallowed per-seed; Wait only returns ctx errors

	// Merge preserving seed order, de-duplicating by media_id.
	seen := map[string]bool{}
	var merged []Candidate
	for _, cs := range results {
		for _, c := range cs {
			if seen[c.MediaID] {
				continue
			}
			seen[c.MediaID] = true
			merged = append(merged, c)
		}
	}
	return merged
}
