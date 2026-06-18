package recs

// filter is a composable predicate. A candidate is kept when the predicate
// returns true. Filters mirror Metrolist's filter set: notExplicit, notVideo,
// notShorts, notBlocked, notRecentImpression, notDuplicateInSection.
type filter func(Candidate) bool

// applyFilters returns the subset of candidates passing every predicate, in
// input order.
func applyFilters(in []Candidate, filters ...filter) []Candidate {
	if len(filters) == 0 {
		return in
	}
	out := in[:0:0] // new backing array; don't alias the caller's slice
	for _, c := range in {
		keep := true
		for _, f := range filters {
			if !f(c) {
				keep = false
				break
			}
		}
		if keep {
			out = append(out, c)
		}
	}
	return out
}

// notExplicit drops explicit candidates. Only active when the user opted in.
func notExplicit(c Candidate) bool { return !c.Explicit }

// notVideo drops video-only candidates (we want audio tracks).
func notVideo(c Candidate) bool { return !c.VideoOnly }

// notShorts drops Shorts.
func notShorts(c Candidate) bool { return !c.IsShort }

// notBlocked drops candidates whose media_id is in the blocked set.
func notBlocked(blocked map[string]bool) filter {
	return func(c Candidate) bool { return !blocked[c.MediaID] }
}

// notRecentImpression drops candidates shown to the user at least `cap` times in
// the recent window (impression fatigue). A zero cap drops anything seen at all.
func notRecentImpression(impressions map[string]int, cap int) filter {
	return func(c Candidate) bool {
		shows, ok := impressions[c.MediaID]
		if !ok {
			return true
		}
		return shows <= cap
	}
}

// notDuplicateInSection drops repeats within a single section build. The
// returned filter is stateful — construct one per section.
func notDuplicateInSection() filter {
	seen := map[string]bool{}
	return func(c Candidate) bool {
		if c.MediaID == "" || seen[c.MediaID] {
			return false
		}
		seen[c.MediaID] = true
		return true
	}
}

// prefFilters returns the filters enabled by the user's preferences. Always
// includes the in-section dedupe; explicit/video/shorts gate on Prefs.
func prefFilters(p Prefs) []filter {
	fs := []filter{notDuplicateInSection()}
	if p.HideExplicit {
		fs = append(fs, notExplicit)
	}
	if p.HideVideo {
		fs = append(fs, notVideo)
	}
	if p.HideShorts {
		fs = append(fs, notShorts)
	}
	return fs
}
