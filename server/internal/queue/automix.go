package queue

import (
	"math/rand"
)

// LikedSong is a minimal projection of a liked track used to seed a local
// automix queue (the "shuffle_liked" seed kind). It mirrors the columns
// returned by the ListLikedSongs query.
type LikedSong struct {
	MediaID    string
	Title      string
	DurationMs int
}

// BuildAutomix builds a shuffled queue from the user's liked songs. This powers
// the "shuffle_liked" seed and the client's offline local-radio fallback: it
// needs no network, only library rows already present on the server.
//
// The input slice is not mutated; shuffling happens on a copy.
func BuildAutomix(liked []LikedSong, rng *rand.Rand) []Item {
	shuffled := make([]LikedSong, len(liked))
	copy(shuffled, liked)
	rng.Shuffle(len(shuffled), func(i, j int) {
		shuffled[i], shuffled[j] = shuffled[j], shuffled[i]
	})

	items := make([]Item, 0, len(shuffled))
	for _, s := range shuffled {
		items = append(items, Item{
			MediaID:    s.MediaID,
			Title:      s.Title,
			DurationMs: s.DurationMs,
		})
	}
	return items
}
