package api

import (
	"encoding/json"
	"errors"
	"net/http"

	"github.com/go-chi/chi/v5"
	"github.com/google/uuid"
	"github.com/iceice666/sunflower/server/internal/auth"
	"github.com/iceice666/sunflower/server/internal/db/gen"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgtype"
)

// playlistResponse is the clean JSON shape for a playlist (no pgtype wrappers).
type playlistResponse struct {
	ID         string             `json:"id"`
	Title      string             `json:"title"`
	SourceType string             `json:"source_type"`
	Version    int64              `json:"version"`
	Items      []playlistItemResp `json:"items,omitempty"`
}

type playlistItemResp struct {
	Position   int    `json:"position"`
	MediaID    string `json:"media_id"`
	Title      string `json:"title"`
	ArtistName string `json:"artist_name"`
	AlbumID    string `json:"album_id,omitempty"`
	DurationMs int    `json:"duration_ms,omitempty"`
}

func toPlaylistResponse(p gen.Playlist) playlistResponse {
	return playlistResponse{
		ID:         uuid.UUID(p.ID.Bytes).String(),
		Title:      p.Title,
		SourceType: p.SourceType,
		Version:    p.Version,
	}
}

// listPlaylists — GET /api/v1/playlists
func (d *Deps) listPlaylists(w http.ResponseWriter, r *http.Request) {
	userID := auth.UserIDFromCtx(r.Context())
	size, offset := pagination(r)
	rows, err := gen.New(d.DB).ListPlaylists(r.Context(), gen.ListPlaylistsParams{
		UserID:     pgtype.UUID{Bytes: userID, Valid: true},
		PageOffset: offset,
		PageSize:   size,
	})
	if err != nil {
		d.Log.Error().Err(err).Msg("playlists: list")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}
	out := make([]playlistResponse, len(rows))
	for i, p := range rows {
		out[i] = toPlaylistResponse(p)
	}
	jsonOK(w, map[string]any{"playlists": out})
}

type createPlaylistRequest struct {
	Title string `json:"title"`
}

// createPlaylist — POST /api/v1/playlists
func (d *Deps) createPlaylist(w http.ResponseWriter, r *http.Request) {
	userID := auth.UserIDFromCtx(r.Context())
	var req createPlaylistRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil || req.Title == "" {
		jsonError(w, "invalid_request", http.StatusBadRequest)
		return
	}
	p, err := gen.New(d.DB).InsertPlaylist(r.Context(), gen.InsertPlaylistParams{
		UserID:     pgtype.UUID{Bytes: userID, Valid: true},
		Title:      req.Title,
		SourceType: "local",
	})
	if err != nil {
		d.Log.Error().Err(err).Msg("playlists: create")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}
	jsonOK(w, toPlaylistResponse(p))
}

// getPlaylist — GET /api/v1/playlists/{id} (with items)
func (d *Deps) getPlaylist(w http.ResponseWriter, r *http.Request) {
	userID := auth.UserIDFromCtx(r.Context())
	id, ok := parsePlaylistID(w, r)
	if !ok {
		return
	}
	q := gen.New(d.DB)
	p, err := q.GetPlaylist(r.Context(), gen.GetPlaylistParams{
		ID:     pgtype.UUID{Bytes: id, Valid: true},
		UserID: pgtype.UUID{Bytes: userID, Valid: true},
	})
	if errors.Is(err, pgx.ErrNoRows) {
		jsonError(w, "not_found", http.StatusNotFound)
		return
	}
	if err != nil {
		d.Log.Error().Err(err).Msg("playlists: get")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}
	items, err := q.ListPlaylistItems(r.Context(), pgtype.UUID{Bytes: id, Valid: true})
	if err != nil {
		d.Log.Error().Err(err).Msg("playlists: list items")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}
	resp := toPlaylistResponse(p)
	resp.Items = make([]playlistItemResp, len(items))
	for i, it := range items {
		entry := playlistItemResp{
			Position:   int(it.Position),
			MediaID:    it.SongMediaID,
			Title:      it.Title,
			ArtistName: it.ArtistName,
		}
		if it.AlbumID.Valid {
			entry.AlbumID = it.AlbumID.String
		}
		if it.DurationMs.Valid {
			entry.DurationMs = int(it.DurationMs.Int32)
		}
		resp.Items[i] = entry
	}
	jsonOK(w, resp)
}

// updatePlaylist — PATCH /api/v1/playlists/{id} (rename)
func (d *Deps) updatePlaylist(w http.ResponseWriter, r *http.Request) {
	userID := auth.UserIDFromCtx(r.Context())
	id, ok := parsePlaylistID(w, r)
	if !ok {
		return
	}
	var req createPlaylistRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil || req.Title == "" {
		jsonError(w, "invalid_request", http.StatusBadRequest)
		return
	}
	p, err := gen.New(d.DB).UpdatePlaylistTitle(r.Context(), gen.UpdatePlaylistTitleParams{
		Title:  req.Title,
		ID:     pgtype.UUID{Bytes: id, Valid: true},
		UserID: pgtype.UUID{Bytes: userID, Valid: true},
	})
	if errors.Is(err, pgx.ErrNoRows) {
		jsonError(w, "not_found", http.StatusNotFound)
		return
	}
	if err != nil {
		d.Log.Error().Err(err).Msg("playlists: update")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}
	jsonOK(w, toPlaylistResponse(p))
}

// deletePlaylist — DELETE /api/v1/playlists/{id}
func (d *Deps) deletePlaylist(w http.ResponseWriter, r *http.Request) {
	userID := auth.UserIDFromCtx(r.Context())
	id, ok := parsePlaylistID(w, r)
	if !ok {
		return
	}
	if err := gen.New(d.DB).DeletePlaylist(r.Context(), gen.DeletePlaylistParams{
		ID:     pgtype.UUID{Bytes: id, Valid: true},
		UserID: pgtype.UUID{Bytes: userID, Valid: true},
	}); err != nil {
		d.Log.Error().Err(err).Msg("playlists: delete")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

type addItemRequest struct {
	MediaID string `json:"media_id"`
}

// addPlaylistItem — POST /api/v1/playlists/{id}/items
func (d *Deps) addPlaylistItem(w http.ResponseWriter, r *http.Request) {
	userID := auth.UserIDFromCtx(r.Context())
	deviceID := auth.DeviceIDFromCtx(r.Context())
	id, ok := parsePlaylistID(w, r)
	if !ok {
		return
	}
	var req addItemRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil || req.MediaID == "" {
		jsonError(w, "invalid_request", http.StatusBadRequest)
		return
	}
	q := gen.New(d.DB)
	// Ownership check: the playlist must belong to the caller.
	if _, err := q.GetPlaylist(r.Context(), gen.GetPlaylistParams{
		ID:     pgtype.UUID{Bytes: id, Valid: true},
		UserID: pgtype.UUID{Bytes: userID, Valid: true},
	}); errors.Is(err, pgx.ErrNoRows) {
		jsonError(w, "not_found", http.StatusNotFound)
		return
	} else if err != nil {
		d.Log.Error().Err(err).Msg("playlists: add-item ownership")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}

	pos, err := q.NextPlaylistPosition(r.Context(), pgtype.UUID{Bytes: id, Valid: true})
	if err != nil {
		d.Log.Error().Err(err).Msg("playlists: next-position")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}
	if err := q.AddPlaylistItem(r.Context(), gen.AddPlaylistItemParams{
		PlaylistID:      pgtype.UUID{Bytes: id, Valid: true},
		Position:        pos,
		SongMediaID:     req.MediaID,
		AddedByDeviceID: pgtype.UUID{Bytes: deviceID, Valid: deviceID != uuid.Nil},
	}); err != nil {
		d.Log.Error().Err(err).Msg("playlists: add-item")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}
	if err := q.BumpPlaylistVersion(r.Context(), gen.BumpPlaylistVersionParams{
		ID:     pgtype.UUID{Bytes: id, Valid: true},
		UserID: pgtype.UUID{Bytes: userID, Valid: true},
	}); err != nil {
		d.Log.Warn().Err(err).Msg("playlists: bump version")
	}
	w.WriteHeader(http.StatusNoContent)
}

// removePlaylistItem — DELETE /api/v1/playlists/{id}/items/{media_id}
func (d *Deps) removePlaylistItem(w http.ResponseWriter, r *http.Request) {
	userID := auth.UserIDFromCtx(r.Context())
	id, ok := parsePlaylistID(w, r)
	if !ok {
		return
	}
	mediaID := chi.URLParam(r, "media_id")
	if mediaID == "" {
		jsonError(w, "invalid_request", http.StatusBadRequest)
		return
	}
	q := gen.New(d.DB)
	if _, err := q.GetPlaylist(r.Context(), gen.GetPlaylistParams{
		ID:     pgtype.UUID{Bytes: id, Valid: true},
		UserID: pgtype.UUID{Bytes: userID, Valid: true},
	}); errors.Is(err, pgx.ErrNoRows) {
		jsonError(w, "not_found", http.StatusNotFound)
		return
	} else if err != nil {
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}
	if err := q.RemovePlaylistItem(r.Context(), gen.RemovePlaylistItemParams{
		PlaylistID:  pgtype.UUID{Bytes: id, Valid: true},
		SongMediaID: mediaID,
	}); err != nil {
		d.Log.Error().Err(err).Msg("playlists: remove-item")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}
	if err := q.BumpPlaylistVersion(r.Context(), gen.BumpPlaylistVersionParams{
		ID:     pgtype.UUID{Bytes: id, Valid: true},
		UserID: pgtype.UUID{Bytes: userID, Valid: true},
	}); err != nil {
		d.Log.Warn().Err(err).Msg("playlists: bump version")
	}
	w.WriteHeader(http.StatusNoContent)
}

// parsePlaylistID extracts and validates the {id} path param.
func parsePlaylistID(w http.ResponseWriter, r *http.Request) (uuid.UUID, bool) {
	id, err := uuid.Parse(chi.URLParam(r, "id"))
	if err != nil {
		jsonError(w, "invalid_id", http.StatusBadRequest)
		return uuid.Nil, false
	}
	return id, true
}
