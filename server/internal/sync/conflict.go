package sync

import "time"

// ResolveLastWriteWins picks the winning value of two timestamped writes to the
// same entity: the one with the later occurred_at wins. Ties favor the incoming
// write (incoming >= existing) so a client re-asserting its state is honored.
//
// This is the rule the server applies to cross-device conflicts (e.g. two
// devices liking/unliking the same track). The likes table enforces it in SQL
// via GREATEST(liked_at); this helper centralizes the policy for any handler
// that needs to compare in Go before writing.
func ResolveLastWriteWins(existing, incoming time.Time) (winner time.Time, incomingWins bool) {
	if !incoming.Before(existing) {
		return incoming, true
	}
	return existing, false
}
