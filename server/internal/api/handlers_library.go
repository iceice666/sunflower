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
	jsonOK(w, map[string]any{"songs": rows})
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
