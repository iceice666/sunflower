// Package events ingests batched play events from the client and applies the
// scrobble threshold that decides which "play" events count toward play history
// (and therefore recommendations).
package events

// scrobbleThresholdMs mirrors Metrolist's PlaybackStatsListener: a play counts
// once the listener has played at least this many milliseconds (or a fraction of
// the track — see Qualifies). 30 s is the common scrobble floor.
const scrobbleThresholdMs = 30_000

// scrobbleFraction is the alternate threshold: half the track duration. A play
// qualifies when it passes EITHER the absolute floor or this fraction, so short
// tracks (< 60 s) still scrobble at the halfway point.
const scrobbleFraction = 0.5

// Qualifies reports whether a play event with totalPlayedMs over a track of
// durationMs should be counted as a scrobble. durationMs <= 0 (unknown) falls
// back to the absolute floor only.
func Qualifies(totalPlayedMs, durationMs int) bool {
	if totalPlayedMs >= scrobbleThresholdMs {
		return true
	}
	if durationMs > 0 {
		return float64(totalPlayedMs) >= float64(durationMs)*scrobbleFraction
	}
	return false
}
