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

// parseResponsiveListSong parses a musicResponsiveListItemRenderer into a SongItem.
// This renderer is used by search, related, and playlist pages.
// It differs from parseSongItem (playlistPanelVideoRenderer) in where fields live.
func parseResponsiveListSong(m map[string]any) models.SongItem {
	if m == nil {
		return models.SongItem{}
	}

	// videoId is in playlistItemData or in the overlay play button.
	videoID := getString(m, "playlistItemData", "videoId")
	if videoID == "" {
		videoID = getString(m, "overlay", "musicItemThumbnailOverlayRenderer",
			"content", "musicPlayButtonRenderer", "playNavigationEndpoint",
			"watchEndpoint", "videoId")
	}

	return models.SongItem{
		VideoID:      videoID,
		Title:        responsiveTitle(m),
		Artists:      responsiveArtists(m),
		ThumbnailURL: responsiveThumbnail(m),
	}
}

func parseAlbumItem(m map[string]any) models.AlbumItem {
	if m == nil {
		return models.AlbumItem{}
	}
	return models.AlbumItem{
		BrowseID:     getString(m, "navigationEndpoint", "browseEndpoint", "browseId"),
		Title:        firstNonEmpty(firstRunText(m, "title"), responsiveTitle(m)),
		Artists:      responsiveArtists(m),
		ThumbnailURL: responsiveThumbnail(m),
	}
}

func parseArtistItem(m map[string]any) models.ArtistItem {
	if m == nil {
		return models.ArtistItem{}
	}
	return models.ArtistItem{
		BrowseID:     getString(m, "navigationEndpoint", "browseEndpoint", "browseId"),
		Name:         firstNonEmpty(firstRunText(m, "title"), responsiveTitle(m)),
		ThumbnailURL: responsiveThumbnail(m),
	}
}

func responsiveTitle(m map[string]any) string {
	flexCols := getArray(m, "flexColumns")
	if len(flexCols) == 0 {
		return ""
	}
	col0, ok := flexCols[0].(map[string]any)
	if !ok {
		return ""
	}
	colRenderer := getMap(col0, "musicResponsiveListItemFlexColumnRenderer")
	if colRenderer == nil {
		return ""
	}
	return firstRunText(colRenderer, "text")
}

func responsiveArtists(m map[string]any) []string {
	var artists []string
	flexCols := getArray(m, "flexColumns")
	if len(flexCols) < 2 {
		return artists
	}
	col1, ok := flexCols[1].(map[string]any)
	if !ok {
		return artists
	}
	colRenderer := getMap(col1, "musicResponsiveListItemFlexColumnRenderer")
	if colRenderer == nil {
		return artists
	}
	for _, run := range getArray(colRenderer, "text", "runs") {
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
	return artists
}

func responsiveThumbnail(m map[string]any) string {
	thumbs := getArray(m, "thumbnail", "musicThumbnailRenderer", "thumbnail", "thumbnails")
	if len(thumbs) == 0 {
		return ""
	}
	if t, ok := thumbs[len(thumbs)-1].(map[string]any); ok {
		return getString(t, "url")
	}
	return ""
}

func firstNonEmpty(values ...string) string {
	for _, v := range values {
		if v != "" {
			return v
		}
	}
	return ""
}
