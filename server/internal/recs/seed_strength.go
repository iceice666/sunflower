package recs

import "math"

// seedStrength is the normalized play count of a candidate's seed, 0..1. It uses
// a logarithmic curve so a handful of plays already register meaningfully while
// heavy-rotation seeds saturate rather than dominate.
//
// norm(n) = ln(1+n) / ln(1+cap), clamped to [0,1]. cap is the play count at
// which strength reaches ~1.0.
const seedStrengthCap = 50

func seedStrength(playCount int) float64 {
	if playCount <= 0 {
		return 0
	}
	v := math.Log1p(float64(playCount)) / math.Log1p(seedStrengthCap)
	if v > 1 {
		return 1
	}
	return v
}
