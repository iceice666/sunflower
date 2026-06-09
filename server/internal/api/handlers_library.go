package api

import (
	"context"
	"encoding/json"
	"net/http"
	"strconv"

	"github.com/iceice666/sunflower/server/internal/db/gen"
	"github.com/iceice666/sunflower/server/internal/jobs"
)

func (d *Deps) startScan(w http.ResponseWriter, r *http.Request) {
	var body struct {
		Roots []string `json:"roots"`
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil || len(body.Roots) == 0 {
		jsonError(w, "invalid_request", http.StatusBadRequest)
		return
	}

	job := d.Jobs.Create()
	go jobs.RunScanJob(context.Background(), d.Jobs, d.Scanner, job.ID, body.Roots)
	jsonOK(w, map[string]string{"job_id": job.ID})
}

// songResponse is the clean JSON shape for a song in the list endpoint.
// All nullable fields are unwrapped from pgtype wrappers so the client gets
// plain JSON values (strings, ints, nulls) rather than {"String":…,"Valid":…}.
type songResponse struct {
	MediaID    string  `json:"media_id"`
	SourceType string  `json:"source_type"`
	Title      string  `json:"title"`
	DurationMs *int32  `json:"duration_ms"`
	AlbumID    *string `json:"album_id"`
	ArtistName string  `json:"artist_name"`
	AlbumTitle string  `json:"album_title"`
	HasArt     bool    `json:"has_art"`
}

func toSongResponse(row gen.ListSongsRow) songResponse {
	resp := songResponse{
		MediaID:    row.MediaID,
		SourceType: row.SourceType,
		Title:      row.Title,
		ArtistName: row.ArtistName,
		AlbumTitle: row.AlbumTitle,
	}
	if row.DurationMs.Valid {
		v := row.DurationMs.Int32
		resp.DurationMs = &v
	}
	if row.AlbumID.Valid {
		v := row.AlbumID.String
		resp.AlbumID = &v
	}
	// has_art is an interface{} from the SQL boolean expression; cast safely.
	if b, ok := row.HasArt.(bool); ok {
		resp.HasArt = b
	}
	return resp
}

func (d *Deps) listSongs(w http.ResponseWriter, r *http.Request) {
	size, offset := pagination(r)
	rows, err := gen.New(d.DB).ListSongs(r.Context(), gen.ListSongsParams{
		PageSize:   size,
		PageOffset: offset,
	})
	if err != nil {
		d.Log.Error().Err(err).Msg("list-songs")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}
	songs := make([]songResponse, len(rows))
	for i, row := range rows {
		songs[i] = toSongResponse(row)
	}
	jsonOK(w, map[string]any{"songs": songs})
}

func (d *Deps) listAlbums(w http.ResponseWriter, r *http.Request) {
	size, offset := pagination(r)
	rows, err := gen.New(d.DB).ListAlbums(r.Context(), gen.ListAlbumsParams{
		PageSize:   size,
		PageOffset: offset,
	})
	if err != nil {
		d.Log.Error().Err(err).Msg("list-albums")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}
	jsonOK(w, map[string]any{"albums": rows})
}

func (d *Deps) listArtists(w http.ResponseWriter, r *http.Request) {
	size, offset := pagination(r)
	rows, err := gen.New(d.DB).ListArtists(r.Context(), gen.ListArtistsParams{
		PageSize:   size,
		PageOffset: offset,
	})
	if err != nil {
		d.Log.Error().Err(err).Msg("list-artists")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}
	jsonOK(w, map[string]any{"artists": rows})
}

// pagination parses ?limit= and ?offset= query params with safe defaults.
func pagination(r *http.Request) (size, offset int32) {
	size = 20
	if v := r.URL.Query().Get("limit"); v != "" {
		if n, err := strconv.Atoi(v); err == nil && n > 0 && n <= 100 {
			size = int32(n)
		}
	}
	if v := r.URL.Query().Get("offset"); v != "" {
		if n, err := strconv.Atoi(v); err == nil && n >= 0 {
			offset = int32(n)
		}
	}
	return size, offset
}
