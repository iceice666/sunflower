package recs

import "testing"

func TestHomeCacheKey_VariesByPrefs(t *testing.T) {
	u := mustUUID("11111111-1111-1111-1111-111111111111")
	base := homeCacheKey(u, Prefs{})
	explicit := homeCacheKey(u, Prefs{HideExplicit: true})
	video := homeCacheKey(u, Prefs{HideVideo: true})
	all := homeCacheKey(u, Prefs{HideExplicit: true, HideVideo: true, HideShorts: true})

	keys := map[string]bool{base: true, explicit: true, video: true, all: true}
	if len(keys) != 4 {
		t.Fatalf("each distinct prefs set must yield a distinct cache key, got %v", keys)
	}
}

func TestHomeCacheKey_VariesByUser(t *testing.T) {
	a := homeCacheKey(mustUUID("11111111-1111-1111-1111-111111111111"), Prefs{})
	b := homeCacheKey(mustUUID("22222222-2222-2222-2222-222222222222"), Prefs{})
	if a == b {
		t.Fatal("different users must have different cache keys")
	}
}

func TestFiltersHash_Stable(t *testing.T) {
	p := Prefs{HideExplicit: true, HideShorts: true}
	if filtersHash(p) != filtersHash(p) {
		t.Fatal("filtersHash must be deterministic")
	}
	if filtersHash(Prefs{}) == filtersHash(Prefs{HideExplicit: true}) {
		t.Fatal("differing prefs must hash differently")
	}
}
