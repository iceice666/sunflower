package queue

import (
	"math/rand"
	"testing"
)

func TestBuildAutomixPreservesAllItems(t *testing.T) {
	liked := make([]LikedSong, 12)
	for i := range liked {
		liked[i] = LikedSong{MediaID: "local:" + string(rune('a'+i)), Title: "t"}
	}

	got := BuildAutomix(liked, rand.New(rand.NewSource(1)))
	if len(got) != len(liked) {
		t.Fatalf("got %d items, want %d (no drops)", len(got), len(liked))
	}
	// Every original media_id must survive the shuffle.
	seen := map[string]bool{}
	for _, it := range got {
		seen[it.MediaID] = true
	}
	for _, l := range liked {
		if !seen[l.MediaID] {
			t.Fatalf("media_id %q missing after shuffle", l.MediaID)
		}
	}
}

func TestBuildAutomixDoesNotMutateInput(t *testing.T) {
	liked := []LikedSong{{MediaID: "local:a"}, {MediaID: "local:b"}, {MediaID: "local:c"}}
	orig := make([]LikedSong, len(liked))
	copy(orig, liked)

	_ = BuildAutomix(liked, rand.New(rand.NewSource(99)))
	for i := range liked {
		if liked[i] != orig[i] {
			t.Fatalf("input mutated at %d: got %v, want %v", i, liked[i], orig[i])
		}
	}
}

func TestBuildAutomixEmpty(t *testing.T) {
	if got := BuildAutomix(nil, rand.New(rand.NewSource(1))); len(got) != 0 {
		t.Fatalf("got %d items, want 0 for empty input", len(got))
	}
}
