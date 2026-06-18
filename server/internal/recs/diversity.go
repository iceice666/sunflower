package recs

// diversifier tracks how many times each artist/album has already appeared in a
// section and yields a diminishing boost for fresh artists/albums. It spreads a
// section so it is not dominated by a single artist or album. Construct one per
// section; call boost() in ranked order as items are committed.
type diversifier struct {
	artists map[string]int
	albums  map[string]int
}

func newDiversifier() *diversifier {
	return &diversifier{artists: map[string]int{}, albums: map[string]int{}}
}

// boost returns a 0..1 diversity score for c given what has been committed so
// far: 1.0 for a never-seen artist and album, decaying as repeats accumulate.
// It does NOT mutate state — call commit() once the item is accepted.
func (d *diversifier) boost(c Candidate) float64 {
	artistKey := firstArtist(c)
	ar := d.artists[artistKey]
	al := d.albums[c.AlbumID]
	// Each prior appearance halves that dimension's contribution.
	artistScore := 1.0 / float64(1+ar)
	albumScore := 1.0
	if c.AlbumID != "" {
		albumScore = 1.0 / float64(1+al)
	}
	return (artistScore + albumScore) / 2
}

// commit records that c was accepted into the section.
func (d *diversifier) commit(c Candidate) {
	d.artists[firstArtist(c)]++
	if c.AlbumID != "" {
		d.albums[c.AlbumID]++
	}
}

func firstArtist(c Candidate) string {
	if len(c.Artists) == 0 {
		return ""
	}
	return c.Artists[0]
}
