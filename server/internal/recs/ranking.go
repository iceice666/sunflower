package recs

import (
	"sort"
	"time"
)

// Ranking weights (plans/architecture.md §internal/recs). They sum to 1.0.
const (
	wSourceAffinity   = 0.35
	wSeedStrength     = 0.20
	wRecency          = 0.15
	wNovelty          = 0.15
	wRemoteConfidence = 0.10
	wDiversityBoost   = 0.05
)

// scoreInputs bundles the per-candidate signals the ranker needs that are not
// already on the Candidate (impression counts, the diversity boost computed in
// committed order).
type scoreInputs struct {
	impressions    int
	diversityBoost float64
}

// score computes the weighted rank for a candidate. diversityBoost is supplied
// separately because it depends on what has already been committed to the
// section, so it cannot be derived from the candidate alone.
func score(c Candidate, in scoreInputs, now time.Time) float64 {
	return wSourceAffinity*sourceAffinity(c) +
		wSeedStrength*seedStrength(c.PlayCount) +
		wRecency*recency(c.LastPlayed, now) +
		wNovelty*novelty(in.impressions) +
		wRemoteConfidence*clamp01(c.RemoteConfidence) +
		wDiversityBoost*clamp01(in.diversityBoost)
}

// rankAndDiversify scores, sorts, and diversity-spreads candidates into a final
// ordered Item list capped at limit. impressions maps media_id → recent show
// count (novelty input). The diversity boost is applied greedily: candidates are
// first ordered by their base score (without diversity), then re-scored with the
// diversity dimension as each is committed, mirroring a greedy MMR-style spread.
func rankAndDiversify(cands []Candidate, impressions map[string]int, now time.Time, limit int) []Item {
	if limit <= 0 || len(cands) == 0 {
		return nil
	}

	// Base score (diversity = 0) for the initial ordering.
	type scored struct {
		c    Candidate
		base float64
	}
	pool := make([]scored, len(cands))
	for i, c := range cands {
		pool[i] = scored{c: c, base: score(c, scoreInputs{impressions: impressions[c.MediaID]}, now)}
	}
	sort.SliceStable(pool, func(i, j int) bool { return pool[i].base > pool[j].base })

	div := newDiversifier()
	out := make([]Item, 0, min(limit, len(pool)))
	used := make([]bool, len(pool))

	for len(out) < limit {
		bestIdx := -1
		bestScore := -1.0
		for i := range pool {
			if used[i] {
				continue
			}
			s := score(pool[i].c, scoreInputs{
				impressions:    impressions[pool[i].c.MediaID],
				diversityBoost: div.boost(pool[i].c),
			}, now)
			if s > bestScore {
				bestScore = s
				bestIdx = i
			}
		}
		if bestIdx < 0 {
			break
		}
		used[bestIdx] = true
		c := pool[bestIdx].c
		div.commit(c)
		out = append(out, Item{
			MediaID:    c.MediaID,
			Title:      c.Title,
			Artists:    c.Artists,
			AlbumID:    c.AlbumID,
			DurationMs: c.DurationMs,
			Source:     c.Source,
			ThumbURL:   c.ThumbURL,
			Score:      bestScore,
		})
	}
	return out
}

func clamp01(v float64) float64 {
	if v < 0 {
		return 0
	}
	if v > 1 {
		return 1
	}
	return v
}

func min(a, b int) int {
	if a < b {
		return a
	}
	return b
}
