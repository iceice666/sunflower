package models

import (
	"github.com/iceice666/sunflower/server/internal/innertube/continuation"
	"time"
)

type Locale struct {
	HL string // e.g. "en"
	GL string // e.g. "US"
}

type StreamURL struct {
	URL       string
	ExpiresAt time.Time
	Itag      int
	MimeType  string
	Bitrate   int
	Loudness  float64 // loudness_normalization_db; zero if absent
}

type SongItem struct {
	VideoID      string
	Title        string
	Artists      []string
	AlbumTitle   string
	DurationMs   int
	ThumbnailURL string
}

type AlbumItem struct {
	BrowseID     string
	Title        string
	Artists      []string
	Year         string
	ThumbnailURL string
}

type ArtistItem struct {
	BrowseID     string
	Name         string
	ThumbnailURL string
}

type PlaylistItem struct {
	BrowseID     string
	Title        string
	ThumbnailURL string
}

type PlayerResponse struct {
	VideoID     string
	PlayerJsURL string // absolute base.js URL; see sig.Cache.Bootstrap for source
	Stream      StreamURL
	AllStreams  []StreamURL
}

type NextPage struct {
	Current      SongItem
	Related      []SongItem
	Continuation continuation.Cursor
}

type HomeSection struct {
	Title string
	Items []any // SongItem | AlbumItem | PlaylistItem
}

type HomePage struct {
	Sections []HomeSection
	Chips    []string
}

type SearchPage struct {
	Songs        []SongItem
	Albums       []AlbumItem
	Artists      []ArtistItem
	Continuation continuation.Cursor
}

// ProbeNextResult is the JSON output of `probe innertube next`.
type ProbeNextResult struct {
	CurrentURL   string              `json:"current_url"`
	ExpiresAt    time.Time           `json:"expires_at"`
	Itag         int                 `json:"itag"`
	NextItems    []SongItem          `json:"next_items"`
	Continuation continuation.Cursor `json:"continuation,omitempty"`
}
