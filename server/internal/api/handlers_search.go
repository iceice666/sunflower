package api

import (
	"context"
	"encoding/json"
	"net/http"
	"strconv"
	"strings"
	"time"

	"github.com/iceice666/sunflower/server/internal/innertube/parser"
)

type searchYT interface {
	Search(ctx context.Context, query string) (json.RawMessage, error)
}

type searchResponse struct {
	Query        string                 `json:"query"`
	Songs        []searchSongResponse   `json:"songs"`
	Albums       []searchAlbumResponse  `json:"albums"`
	Artists      []searchArtistResponse `json:"artists"`
	Continuation string                 `json:"continuation,omitempty"`
}

type searchSongResponse struct {
	MediaID      string   `json:"media_id"`
	Source       string   `json:"source"`
	Title        string   `json:"title"`
	Artists      []string `json:"artists,omitempty"`
	ThumbnailURL string   `json:"thumbnail_url,omitempty"`
	DurationMs   int      `json:"duration_ms"`
}

type searchAlbumResponse struct {
	BrowseID     string   `json:"browse_id"`
	Title        string   `json:"title"`
	Artists      []string `json:"artists,omitempty"`
	ThumbnailURL string   `json:"thumbnail_url,omitempty"`
}

type searchArtistResponse struct {
	BrowseID     string `json:"browse_id"`
	Name         string `json:"name"`
	ThumbnailURL string `json:"thumbnail_url,omitempty"`
}

// search returns YouTube Music search results.
//
// GET /api/v1/search?q=<query>&limit=<1..25>
func (d *Deps) search(w http.ResponseWriter, r *http.Request) {
	query := strings.TrimSpace(r.URL.Query().Get("q"))
	if len(query) < 2 {
		jsonError(w, "invalid_query", http.StatusBadRequest)
		return
	}

	yt, ok := d.YT.(searchYT)
	if d.YT == nil || !ok {
		jsonError(w, "yt_unavailable", http.StatusServiceUnavailable)
		return
	}

	limit := searchLimit(r)
	ctx, cancel := context.WithTimeout(r.Context(), 8*time.Second)
	defer cancel()

	raw, err := yt.Search(ctx, query)
	if err != nil {
		d.Log.Warn().Err(err).Str("query", query).Msg("search: innertube")
		jsonError(w, "search_unavailable", http.StatusBadGateway)
		return
	}

	page := parser.ParseSearchPage(raw)
	out := searchResponse{
		Query:        query,
		Songs:        make([]searchSongResponse, 0, min(len(page.Songs), limit)),
		Albums:       make([]searchAlbumResponse, 0, min(len(page.Albums), limit)),
		Artists:      make([]searchArtistResponse, 0, min(len(page.Artists), limit)),
		Continuation: string(page.Continuation),
	}
	for _, song := range page.Songs {
		if len(out.Songs) >= limit {
			break
		}
		if song.VideoID == "" || song.Title == "" {
			continue
		}
		out.Songs = append(out.Songs, searchSongResponse{
			MediaID:      "yt:" + strings.TrimPrefix(song.VideoID, "yt:"),
			Source:       "yt",
			Title:        song.Title,
			Artists:      song.Artists,
			ThumbnailURL: song.ThumbnailURL,
			DurationMs:   song.DurationMs,
		})
	}
	for _, album := range page.Albums {
		if len(out.Albums) >= limit {
			break
		}
		if album.BrowseID == "" || album.Title == "" {
			continue
		}
		out.Albums = append(out.Albums, searchAlbumResponse{
			BrowseID:     album.BrowseID,
			Title:        album.Title,
			Artists:      album.Artists,
			ThumbnailURL: album.ThumbnailURL,
		})
	}
	for _, artist := range page.Artists {
		if len(out.Artists) >= limit {
			break
		}
		if artist.BrowseID == "" || artist.Name == "" {
			continue
		}
		out.Artists = append(out.Artists, searchArtistResponse{
			BrowseID:     artist.BrowseID,
			Name:         artist.Name,
			ThumbnailURL: artist.ThumbnailURL,
		})
	}

	jsonOK(w, out)
}

func searchLimit(r *http.Request) int {
	limit := 20
	if v := r.URL.Query().Get("limit"); v != "" {
		if n, err := strconv.Atoi(v); err == nil && n > 0 {
			limit = n
		}
	}
	if limit > 25 {
		return 25
	}
	return limit
}
