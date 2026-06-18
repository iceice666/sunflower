// server/internal/innertube/parser/yt_item.go
package parser

import (
	"github.com/iceice666/sunflower/server/internal/innertube/models"
)

func parseSongItem(m map[string]any) models.SongItem {
	if m == nil {
		return models.SongItem{}
	}
	// title.runs[0].text — must use firstRunText because getString cannot
	// traverse []any with numeric string keys.
	title := firstRunText(m, "title")

	videoID := getString(m, "videoId")

	var artists []string
	for _, run := range getArray(m, "subtitle", "runs") {
		r, ok := run.(map[string]any)
		if !ok {
			continue
		}
		ep := getMap(r, "navigationEndpoint", "browseEndpoint")
		if ep == nil {
			continue
		}
		pageType := getString(ep, "browseEndpointContextSupportedConfigs",
			"browseEndpointContextMusicConfig", "pageType")
		if pageType == "MUSIC_PAGE_TYPE_ARTIST" {
			artists = append(artists, getString(r, "text"))
		}
	}

	thumbnail := ""
	thumbs := getArray(m, "thumbnail", "thumbnails")
	if len(thumbs) > 0 {
		if t, ok := thumbs[len(thumbs)-1].(map[string]any); ok {
			thumbnail = getString(t, "url")
		}
	}

	return models.SongItem{
		VideoID:      videoID,
		Title:        title,
		Artists:      artists,
		ThumbnailURL: thumbnail,
	}
}

func parseAlbumItem(m map[string]any) models.AlbumItem {
	if m == nil {
		return models.AlbumItem{}
	}
	return models.AlbumItem{
		BrowseID: getString(m, "navigationEndpoint", "browseEndpoint", "browseId"),
		Title:    firstRunText(m, "title"),
	}
}

func parseArtistItem(m map[string]any) models.ArtistItem {
	if m == nil {
		return models.ArtistItem{}
	}
	return models.ArtistItem{
		BrowseID: getString(m, "navigationEndpoint", "browseEndpoint", "browseId"),
		Name:     firstRunText(m, "title"),
	}
}
