package parser_test

import (
	"os"
	"testing"

	"github.com/iceice666/sunflower/server/internal/innertube/parser"
)

func TestParseHomePage_EmptyJSON(t *testing.T) {
	page := parser.ParseHomePage([]byte("{}"))
	// Must not panic.
	_ = page
}

func TestParseHomePage_FromFixture(t *testing.T) {
	raw, err := os.ReadFile("testdata/home_response.json")
	if err != nil {
		t.Fatalf("fixture missing: %v", err)
	}
	page := parser.ParseHomePage(raw)
	if len(page.Sections) == 0 {
		t.Error("expected at least one section")
	}
	t.Logf("sections: %d, chips: %d", len(page.Sections), len(page.Chips))
}

func TestParseSearchPage_EmptyJSON(t *testing.T) {
	page := parser.ParseSearchPage([]byte("{}"))
	_ = page
}

func TestParseSearchPage_FromFixture(t *testing.T) {
	raw, err := os.ReadFile("testdata/search_response.json")
	if err != nil {
		t.Fatalf("fixture missing: %v", err)
	}
	page := parser.ParseSearchPage(raw)
	if len(page.Songs) == 0 {
		t.Error("expected at least one song")
	}
	if page.Songs[0].VideoID == "" {
		t.Error("song VideoID should not be empty")
	}
	if page.Songs[0].Title == "" {
		t.Error("song Title should not be empty")
	}
	t.Logf("songs: %d, albums: %d, artists: %d", len(page.Songs), len(page.Albums), len(page.Artists))
}

func TestParseRelatedPage_EmptyJSON(t *testing.T) {
	items := parser.ParseRelatedPage([]byte("{}"))
	_ = items
}

func TestParseArtistPage_EmptyJSON(t *testing.T) {
	item := parser.ParseArtistPage([]byte("{}"))
	_ = item
}

func TestParseAlbumPage_EmptyJSON(t *testing.T) {
	item := parser.ParseAlbumPage([]byte("{}"))
	_ = item
}

func TestParsePlaylistPage_EmptyJSON(t *testing.T) {
	items := parser.ParsePlaylistPage([]byte("{}"))
	_ = items
}
