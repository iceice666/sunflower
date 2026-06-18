package recs

import (
	"testing"
	"time"
)

func TestWeightsSumToOne(t *testing.T) {
	sum := wSourceAffinity + wSeedStrength + wRecency + wNovelty +
		wRemoteConfidence + wDiversityBoost
	if d := sum - 1.0; d > 1e-9 || d < -1e-9 {
		t.Fatalf("ranking weights must sum to 1.0, got %v", sum)
	}
}

// TestRanking_SeedStrengthOrders verifies the m5 acceptance test: vary one
// signal (here seed strength via play count) with all else equal, and the
// ordering follows that signal. Two local tracks identical except play count →
// the higher play count ranks first.
func TestRanking_SeedStrengthOrders(t *testing.T) {
	now := time.Unix(1_700_000_000, 0)
	cands := []Candidate{
		{MediaID: "local:low", Source: "local", PlayCount: 1, Artists: []string{"A"}},
		{MediaID: "local:high", Source: "local", PlayCount: 40, Artists: []string{"B"}},
	}
	items := rankAndDiversify(cands, nil, now, 2)
	if len(items) != 2 {
		t.Fatalf("want 2 items, got %d", len(items))
	}
	if items[0].MediaID != "local:high" {
		t.Fatalf("higher play count should rank first, got %q", items[0].MediaID)
	}
}

// TestRanking_RecencyOrders varies only recency.
func TestRanking_RecencyOrders(t *testing.T) {
	now := time.Unix(1_700_000_000, 0)
	cands := []Candidate{
		{MediaID: "local:old", Source: "local", PlayCount: 10, LastPlayed: now.Add(-60 * 24 * time.Hour), Artists: []string{"A"}},
		{MediaID: "local:new", Source: "local", PlayCount: 10, LastPlayed: now.Add(-1 * time.Hour), Artists: []string{"B"}},
	}
	items := rankAndDiversify(cands, nil, now, 2)
	if items[0].MediaID != "local:new" {
		t.Fatalf("more recent should rank first, got %q", items[0].MediaID)
	}
}

// TestRanking_SourceAffinityOrders varies only source.
func TestRanking_SourceAffinityOrders(t *testing.T) {
	now := time.Unix(1_700_000_000, 0)
	cands := []Candidate{
		{MediaID: "yt:x", Source: "yt", Artists: []string{"A"}},
		{MediaID: "local:y", Source: "local", Artists: []string{"B"}},
	}
	items := rankAndDiversify(cands, nil, now, 2)
	if items[0].MediaID != "local:y" {
		t.Fatalf("local source has higher affinity, want local first, got %q", items[0].MediaID)
	}
}

// TestRanking_NoveltyPenalizesSeen varies only impression count.
func TestRanking_NoveltyPenalizesSeen(t *testing.T) {
	now := time.Unix(1_700_000_000, 0)
	cands := []Candidate{
		{MediaID: "local:seen", Source: "local", PlayCount: 5, Artists: []string{"A"}},
		{MediaID: "local:fresh", Source: "local", PlayCount: 5, Artists: []string{"B"}},
	}
	impr := map[string]int{"local:seen": noveltyCap} // fully seen → novelty 0
	items := rankAndDiversify(cands, impr, now, 2)
	if items[0].MediaID != "local:fresh" {
		t.Fatalf("unseen item should rank above over-shown item, got %q", items[0].MediaID)
	}
}

// TestRanking_DiversitySpread ensures the section is not dominated by one artist:
// with three tracks from artist A and one from B, B should be lifted above the
// later A tracks once A repeats accrue diversity penalty.
func TestRanking_DiversitySpread(t *testing.T) {
	now := time.Unix(1_700_000_000, 0)
	cands := []Candidate{
		{MediaID: "local:a1", Source: "local", PlayCount: 30, Artists: []string{"A"}},
		{MediaID: "local:a2", Source: "local", PlayCount: 29, Artists: []string{"A"}},
		{MediaID: "local:a3", Source: "local", PlayCount: 28, Artists: []string{"A"}},
		{MediaID: "local:b1", Source: "local", PlayCount: 20, Artists: []string{"B"}},
	}
	items := rankAndDiversify(cands, nil, now, 4)
	// a1 wins outright. b1 should appear before a3 thanks to diversity boost.
	pos := map[string]int{}
	for i, it := range items {
		pos[it.MediaID] = i
	}
	if pos["local:b1"] > pos["local:a3"] {
		t.Fatalf("diversity should lift artist B above the third A track: %+v", pos)
	}
}

// TestSubScorers spot-checks the individual scorers' boundaries.
func TestSubScorers(t *testing.T) {
	if v := seedStrength(0); v != 0 {
		t.Errorf("seedStrength(0)=%v want 0", v)
	}
	if v := seedStrength(seedStrengthCap); v < 0.99 || v > 1.0 {
		t.Errorf("seedStrength(cap)=%v want ~1.0", v)
	}
	if v := novelty(0); v != 1 {
		t.Errorf("novelty(0)=%v want 1", v)
	}
	if v := novelty(noveltyCap); v != 0 {
		t.Errorf("novelty(cap)=%v want 0", v)
	}
	now := time.Unix(1_700_000_000, 0)
	if v := recency(time.Time{}, now); v != 0 {
		t.Errorf("recency(zero)=%v want 0", v)
	}
	half := recency(now.Add(-recencyHalfLife), now)
	if half < 0.49 || half > 0.51 {
		t.Errorf("recency(halfLife)=%v want ~0.5", half)
	}
	if sourceAffinity(Candidate{Source: "local"}) <= sourceAffinity(Candidate{Source: "yt"}) {
		t.Error("local affinity should exceed yt affinity")
	}
}
