// Package library scans local music directories and populates the database.
package library

import (
	"crypto/sha1"
	"encoding/hex"
	"os"
	"path/filepath"
	"strings"

	"github.com/dhowden/tag"
)

// TrackMeta holds extracted metadata for a single audio file.
type TrackMeta struct {
	MediaID       string
	Title         string
	Artist        string
	ArtistMediaID string
	Album         string
	AlbumMediaID  string
	Year          int
	TrackNum      int
	DurationMs    int // left zero — dhowden/tag does not expose stream duration
	Path          string
	Picture       *tag.Picture // may be nil
}

var audioExts = map[string]bool{
	".mp3": true, ".flac": true, ".m4a": true,
	".ogg": true, ".opus": true,
}

// IsAudioFile reports whether the file extension is a supported audio format.
func IsAudioFile(path string) bool {
	return audioExts[strings.ToLower(filepath.Ext(path))]
}

// localMediaID returns "local:" + first 16 hex chars of SHA-1(input).
func localMediaID(input string) string {
	sum := sha1.Sum([]byte(input))
	return "local:" + hex.EncodeToString(sum[:])[:16]
}

// ExtractTags opens path and extracts audio metadata.
// Returns a non-nil TrackMeta on success; missing tag fields produce zero values.
func ExtractTags(path string) (*TrackMeta, error) {
	abs, err := filepath.Abs(path)
	if err != nil {
		return nil, err
	}

	f, err := os.Open(abs)
	if err != nil {
		return nil, err
	}
	defer f.Close()

	m, err := tag.ReadFrom(f)
	if err != nil {
		return nil, err
	}

	title := m.Title()
	if title == "" {
		title = strings.TrimSuffix(filepath.Base(abs), filepath.Ext(abs))
	}
	artist := m.Artist()
	album := m.Album()
	year := m.Year()
	track, _ := m.Track()

	return &TrackMeta{
		MediaID:       localMediaID(abs),
		Title:         title,
		Artist:        artist,
		ArtistMediaID: localMediaID("a:" + strings.ToLower(artist)),
		Album:         album,
		AlbumMediaID:  localMediaID("l:" + strings.ToLower(album) + "|" + strings.ToLower(artist)),
		Year:          year,
		TrackNum:      track,
		Path:          abs,
		Picture:       m.Picture(),
	}, nil
}
