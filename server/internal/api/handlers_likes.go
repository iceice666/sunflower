package api

import (
	"encoding/json"
	"net/http"
	"time"

	"github.com/google/uuid"
	"github.com/iceice666/sunflower/server/internal/auth"
	"github.com/iceice666/sunflower/server/internal/db/gen"
	"github.com/jackc/pgx/v5/pgtype"
)

// likeRequest is the POST /api/v1/likes body. liked=true adds the like;
// liked=false removes it. occurred_at drives last-write-wins; absent → now.
type likeRequest struct {
	MediaID    string  `json:"media_id"`
	Liked      bool    `json:"liked"`
	OccurredAt *string `json:"occurred_at"` // RFC3339; optional
}

type likeResponse struct {
	MediaID string `json:"media_id"`
	Liked   bool   `json:"liked"`
}

// postLike toggles a like. Idempotent via the Idempotency-Key header (UUIDv7):
// the key is stored on the like row so replays don't double-apply. Conflict
// resolution is last-write-wins by occurred_at (the SQL GREATEST on liked_at).
//
// POST /api/v1/likes
func (d *Deps) postLike(w http.ResponseWriter, r *http.Request) {
	userID := auth.UserIDFromCtx(r.Context())

	var req likeRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil || req.MediaID == "" {
		jsonError(w, "invalid_request", http.StatusBadRequest)
		return
	}

	occurred := time.Now().UTC()
	if req.OccurredAt != nil {
		if t, err := time.Parse(time.RFC3339, *req.OccurredAt); err == nil {
			occurred = t.UTC()
		}
	}

	q := gen.New(d.DB)
	if !req.Liked {
		if err := q.DeleteLike(r.Context(), gen.DeleteLikeParams{
			UserID:      pgtype.UUID{Bytes: userID, Valid: true},
			SongMediaID: req.MediaID,
		}); err != nil {
			d.Log.Error().Err(err).Msg("likes: delete")
			jsonError(w, "internal", http.StatusInternalServerError)
			return
		}
		jsonOK(w, likeResponse{MediaID: req.MediaID, Liked: false})
		return
	}

	idemKey := idempotencyKeyFrom(r)
	if _, err := q.UpsertLike(r.Context(), gen.UpsertLikeParams{
		UserID:         pgtype.UUID{Bytes: userID, Valid: true},
		SongMediaID:    req.MediaID,
		LikedAt:        pgtype.Timestamptz{Time: occurred, Valid: true},
		IdempotencyKey: idemKey,
	}); err != nil {
		d.Log.Error().Err(err).Msg("likes: upsert")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}
	jsonOK(w, likeResponse{MediaID: req.MediaID, Liked: true})
}

// idempotencyKeyFrom parses the Idempotency-Key header into a pgtype.UUID,
// generating a fresh UUIDv7 when absent or unparseable. (Full middleware-level
// dedupe lands in M7; here the key simply makes the like row replay-safe.)
func idempotencyKeyFrom(r *http.Request) pgtype.UUID {
	if h := r.Header.Get("Idempotency-Key"); h != "" {
		if id, err := uuid.Parse(h); err == nil {
			return pgtype.UUID{Bytes: id, Valid: true}
		}
	}
	id, err := uuid.NewV7()
	if err != nil {
		return pgtype.UUID{} // null; the (user,song) PK still dedupes
	}
	return pgtype.UUID{Bytes: id, Valid: true}
}
