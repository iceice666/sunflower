package sync

import (
	"testing"
	"time"
)

func TestResolveLastWriteWins(t *testing.T) {
	base := time.Unix(1_700_000_000, 0)
	later := base.Add(time.Hour)

	// Later incoming wins.
	got, incomingWins := ResolveLastWriteWins(base, later)
	if !incomingWins || !got.Equal(later) {
		t.Fatalf("later incoming should win: got=%v incomingWins=%v", got, incomingWins)
	}

	// Earlier incoming loses.
	got, incomingWins = ResolveLastWriteWins(later, base)
	if incomingWins || !got.Equal(later) {
		t.Fatalf("earlier incoming should lose: got=%v incomingWins=%v", got, incomingWins)
	}

	// Tie favors incoming (client re-asserting state).
	got, incomingWins = ResolveLastWriteWins(base, base)
	if !incomingWins || !got.Equal(base) {
		t.Fatalf("tie should favor incoming: got=%v incomingWins=%v", got, incomingWins)
	}
}
