package api

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"os"
	"time"

	"github.com/go-chi/chi/v5"
	"github.com/google/uuid"
	"github.com/iceice666/sunflower/server/internal/auth"
	"github.com/iceice666/sunflower/server/internal/db/gen"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgtype"
)

// registerDownloadRequest is the POST /devices/{id}/downloads body.
type registerDownloadRequest struct {
	MediaID   string `json:"media_id"`
	LocalPath string `json:"local_path"`
	Bytes     int64  `json:"bytes"`
}

// registerDownload records that a device has a track downloaded locally.
//
// POST /api/v1/devices/{id}/downloads
//
// The {id} path param must match the authenticated device — a device may only
// register its own downloads.
func (d *Deps) registerDownload(w http.ResponseWriter, r *http.Request) {
	deviceID, ok := d.authorizedDevice(w, r)
	if !ok {
		return
	}
	var req registerDownloadRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil || req.MediaID == "" {
		jsonError(w, "invalid_request", http.StatusBadRequest)
		return
	}

	now := time.Now().UTC()
	var bytes pgtype.Int8
	if req.Bytes > 0 {
		bytes = pgtype.Int8{Int64: req.Bytes, Valid: true}
	}
	if _, err := gen.New(d.DB).UpsertDownload(r.Context(), gen.UpsertDownloadParams{
		DeviceID:       pgtype.UUID{Bytes: deviceID, Valid: true},
		SongMediaID:    req.MediaID,
		LocalPath:      req.LocalPath,
		Bytes:          bytes,
		CompletedAt:    pgtype.Timestamptz{Time: now, Valid: true},
		LastVerifiedAt: pgtype.Timestamptz{Time: now, Valid: true},
	}); err != nil {
		d.Log.Error().Err(err).Msg("downloads: upsert")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

// listDownloads returns the device's registered downloads.
//
// GET /api/v1/devices/{id}/downloads
func (d *Deps) listDownloads(w http.ResponseWriter, r *http.Request) {
	deviceID, ok := d.authorizedDevice(w, r)
	if !ok {
		return
	}
	rows, err := gen.New(d.DB).ListDownloadsForDevice(r.Context(),
		pgtype.UUID{Bytes: deviceID, Valid: true})
	if err != nil {
		d.Log.Error().Err(err).Msg("downloads: list")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}
	type dlResp struct {
		MediaID string `json:"media_id"`
		Bytes   int64  `json:"bytes,omitempty"`
	}
	out := make([]dlResp, len(rows))
	for i, row := range rows {
		out[i] = dlResp{MediaID: row.SongMediaID}
		if row.Bytes.Valid {
			out[i].Bytes = row.Bytes.Int64
		}
	}
	jsonOK(w, map[string]any{"downloads": out})
}

// deleteDownload removes a download registration.
//
// DELETE /api/v1/devices/{id}/downloads/{media_id}
func (d *Deps) deleteDownload(w http.ResponseWriter, r *http.Request) {
	deviceID, ok := d.authorizedDevice(w, r)
	if !ok {
		return
	}
	mediaID := chi.URLParam(r, "media_id")
	if mediaID == "" {
		jsonError(w, "invalid_request", http.StatusBadRequest)
		return
	}
	if err := gen.New(d.DB).DeleteDownload(r.Context(), gen.DeleteDownloadParams{
		DeviceID:    pgtype.UUID{Bytes: deviceID, Valid: true},
		SongMediaID: mediaID,
	}); err != nil {
		d.Log.Error().Err(err).Msg("downloads: delete")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

// songHash returns the SHA-256 of a local song file so the client can verify a
// completed download.
//
// GET /api/v1/library/songs/{media_id}/hash
//
// Only local-library songs have a server-side file to hash; YouTube tracks
// return 404 (their downloads are best-effort, unverified per the M6 spec).
func (d *Deps) songHash(w http.ResponseWriter, r *http.Request) {
	mediaID := chi.URLParam(r, "media_id")
	row, err := gen.New(d.DB).GetSongHashInfo(r.Context(), mediaID)
	if errors.Is(err, pgx.ErrNoRows) {
		jsonError(w, "not_found", http.StatusNotFound)
		return
	}
	if err != nil {
		d.Log.Error().Err(err).Str("media_id", mediaID).Msg("downloads: hash lookup")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}
	if !row.LocalPath.Valid || row.LocalPath.String == "" {
		jsonError(w, "not_local", http.StatusNotFound)
		return
	}

	f, err := os.Open(row.LocalPath.String)
	if errors.Is(err, os.ErrNotExist) {
		jsonError(w, "not_found", http.StatusNotFound)
		return
	}
	if err != nil {
		d.Log.Error().Err(err).Msg("downloads: open for hash")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}
	defer f.Close()

	h := sha256.New()
	size, err := io.Copy(h, f)
	if err != nil {
		d.Log.Error().Err(err).Msg("downloads: hash read")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}
	jsonOK(w, map[string]any{
		"media_id": mediaID,
		"sha256":   hex.EncodeToString(h.Sum(nil)),
		"bytes":    size,
	})
}

// authorizedDevice parses the {id} path param and asserts it matches the
// authenticated device. Returns (deviceID, true) on success; writes an error and
// returns false otherwise.
func (d *Deps) authorizedDevice(w http.ResponseWriter, r *http.Request) (uuid.UUID, bool) {
	pathID, err := uuid.Parse(chi.URLParam(r, "id"))
	if err != nil {
		jsonError(w, "invalid_id", http.StatusBadRequest)
		return uuid.Nil, false
	}
	authed := auth.DeviceIDFromCtx(r.Context())
	if pathID != authed {
		jsonError(w, "forbidden", http.StatusForbidden)
		return uuid.Nil, false
	}
	return authed, true
}
