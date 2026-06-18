package recs

import (
	"math"
	"time"
)

// recencyHalfLife is the age at which a candidate's recency score halves. A
// 14-day half-life keeps the last fortnight's listening prominent while older
// plays decay smoothly toward zero.
const recencyHalfLife = 14 * 24 * time.Hour

// recency returns an exponential-decay score in 0..1 for how recently the
// candidate was last played, relative to now. A zero LastPlayed (never played)
// scores 0.
func recency(lastPlayed, now time.Time) float64 {
	if lastPlayed.IsZero() {
		return 0
	}
	age := now.Sub(lastPlayed)
	if age <= 0 {
		return 1
	}
	// 0.5 ^ (age / halfLife)
	return math.Pow(0.5, float64(age)/float64(recencyHalfLife))
}
