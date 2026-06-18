package recs

import "testing"

func TestApplyFilters_Composable(t *testing.T) {
	in := []Candidate{
		{MediaID: "a", Explicit: true},
		{MediaID: "b", VideoOnly: true},
		{MediaID: "c", IsShort: true},
		{MediaID: "d"}, // clean
	}
	out := applyFilters(in, notExplicit, notVideo, notShorts)
	if len(out) != 1 || out[0].MediaID != "d" {
		t.Fatalf("want only clean candidate d, got %+v", out)
	}
}

func TestApplyFilters_DoesNotMutateInput(t *testing.T) {
	in := []Candidate{{MediaID: "a", Explicit: true}, {MediaID: "b"}}
	_ = applyFilters(in, notExplicit)
	if len(in) != 2 || in[0].MediaID != "a" {
		t.Fatalf("applyFilters mutated caller slice: %+v", in)
	}
}

func TestNotBlocked(t *testing.T) {
	blocked := map[string]bool{"x": true}
	f := notBlocked(blocked)
	if f(Candidate{MediaID: "x"}) {
		t.Error("blocked media should be filtered out")
	}
	if !f(Candidate{MediaID: "y"}) {
		t.Error("unblocked media should pass")
	}
}

func TestNotRecentImpression(t *testing.T) {
	impr := map[string]int{"seen3": 3, "seen1": 1}
	f := notRecentImpression(impr, 2) // drop items shown > 2 times
	if f(Candidate{MediaID: "seen3"}) {
		t.Error("item shown 3 times (>cap 2) should be dropped")
	}
	if !f(Candidate{MediaID: "seen1"}) {
		t.Error("item shown 1 time (<=cap 2) should pass")
	}
	if !f(Candidate{MediaID: "unseen"}) {
		t.Error("unseen item should pass")
	}
}

func TestNotDuplicateInSection(t *testing.T) {
	f := notDuplicateInSection()
	if !f(Candidate{MediaID: "a"}) {
		t.Error("first occurrence should pass")
	}
	if f(Candidate{MediaID: "a"}) {
		t.Error("second occurrence should be dropped")
	}
	if f(Candidate{MediaID: ""}) {
		t.Error("empty media id should be dropped")
	}
}

func TestPrefFilters_GateOnPrefs(t *testing.T) {
	// All prefs off → only the in-section dedupe is active.
	off := prefFilters(Prefs{})
	if len(off) != 1 {
		t.Fatalf("no prefs → only dedupe filter, got %d", len(off))
	}
	// All on → dedupe + 3 toggles.
	on := prefFilters(Prefs{HideExplicit: true, HideVideo: true, HideShorts: true})
	if len(on) != 4 {
		t.Fatalf("all prefs → 4 filters, got %d", len(on))
	}
	// Verify explicit is actually filtered when enabled.
	in := []Candidate{{MediaID: "x", Explicit: true}, {MediaID: "y"}}
	out := applyFilters(in, on...)
	if len(out) != 1 || out[0].MediaID != "y" {
		t.Fatalf("hide_explicit should drop explicit item, got %+v", out)
	}
}
