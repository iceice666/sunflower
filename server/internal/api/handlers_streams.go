package api

import (
	"errors"
	"net/http"
	"os"
	"path/filepath"
	"strconv"

	"github.com/go-chi/chi/v5"
	"github.com/iceice666/sunflower/server/internal/db/gen"
	"github.com/jackc/pgx/v5"
)

// streamSong serves the raw audio bytes for a local song.
//
// GET /api/v1/library/songs/{media_id}/stream
//
// Uses http.ServeFile for Range/206 support — just_audio and ExoPlayer rely on
// byte-range requests for seeking. The auth middleware runs before this handler,
// so every Range sub-request incurs one argon2id hash (~50–100 ms). This is
// acceptable at M2 scale; a token→device cache is the planned M8 mitigation.
func (d *Deps) streamSong(w http.ResponseWriter, r *http.Request) {
	mediaID := chi.URLParam(r, "media_id")

	row, err := gen.New(d.DB).GetSongStream(r.Context(), mediaID)
	if errors.Is(err, pgx.ErrNoRows) {
		jsonError(w, "not_found", http.StatusNotFound)
		return
	}
	if err != nil {
		d.Log.Error().Err(err).Str("media_id", mediaID).Msg("get-song-stream")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}
	if !row.LocalPath.Valid || row.LocalPath.String == "" {
		// Song exists but has no local path (e.g. a YouTube track).
		jsonError(w, "not_found", http.StatusNotFound)
		return
	}

	path := row.LocalPath.String
	if _, err := os.Stat(path); errors.Is(err, os.ErrNotExist) {
		// File was deleted from disk after the last scan.
		jsonError(w, "not_found", http.StatusNotFound)
		return
	}

	// Set Content-Type by extension so just_audio gets a reliable MIME type.
	switch filepath.Ext(path) {
	case ".mp3":
		w.Header().Set("Content-Type", "audio/mpeg")
	case ".flac":
		w.Header().Set("Content-Type", "audio/flac")
	case ".m4a":
		w.Header().Set("Content-Type", "audio/mp4")
	case ".ogg":
		w.Header().Set("Content-Type", "audio/ogg")
	case ".opus":
		w.Header().Set("Content-Type", "audio/ogg")
	}

	http.ServeFile(w, r, path)
}

// serveAlbumArt serves a cached album cover JPEG.
//
// GET /api/v1/library/albums/{album_media_id}/art?size=256|512|1024
//
// Art files are written by the library scanner to:
//
//	<DataDir>/art/<album_media_id>/<size>.jpg
//
// The recommended client approach for lock-screen art (MediaItem.artUri) is to
// download the art once to a local cache file and hand audio_service a file://
// URI — the OS lock-screen loader may not send Authorization headers.
func (d *Deps) serveAlbumArt(w http.ResponseWriter, r *http.Request) {
	albumMediaID := chi.URLParam(r, "album_media_id")

	size := 512
	if s := r.URL.Query().Get("size"); s != "" {
		n, err := strconv.Atoi(s)
		if err != nil || (n != 256 && n != 512 && n != 1024) {
			jsonError(w, "invalid_size", http.StatusBadRequest)
			return
		}
		size = n
	}

	artPath := filepath.Join(d.DataDir, "art", albumMediaID, strconv.Itoa(size)+".jpg")
	if _, err := os.Stat(artPath); errors.Is(err, os.ErrNotExist) {
		jsonError(w, "not_found", http.StatusNotFound)
		return
	}

	w.Header().Set("Content-Type", "image/jpeg")
	http.ServeFile(w, r, artPath)
}
