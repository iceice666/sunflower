package api

import (
	"encoding/json"
	"errors"
	"math/rand"
	"net/http"
	"strings"
	"time"

	"github.com/go-chi/chi/v5"
	"github.com/google/uuid"
	"github.com/iceice666/sunflower/server/internal/auth"
	"github.com/iceice666/sunflower/server/internal/db/gen"
	"github.com/iceice666/sunflower/server/internal/queue"
	"github.com/jackc/pgx/v5/pgtype"
)

// minQueueItems is the floor for a freshly materialized queue. The acceptance
// criteria require ≥10 pre-materialized items for a YouTube song seed.
const minQueueItems = 10

// startQueueRequest is the POST /queue/start body.
//
// seed_kind is one of:
//   - "song"          → YouTube radio seeded by seed_id ("yt:<videoID>" or "<videoID>")
//   - "shuffle_liked" → shuffled queue from the user's liked songs (no network)
type startQueueRequest struct {
	SeedKind string `json:"seed_kind"`
	SeedID   string `json:"seed_id"`
	Title    string `json:"title"`
}

type queueItemResponse struct {
	MediaID    string   `json:"media_id"`
	Title      string   `json:"title"`
	Artists    []string `json:"artists,omitempty"`
	DurationMs int      `json:"duration_ms"`
}

type queueResponse struct {
	QueueID  string              `json:"queue_id"`
	SeedKind string              `json:"seed_kind"`
	Title    string              `json:"title,omitempty"`
	Version  int64               `json:"version"`
	Items    []queueItemResponse `json:"items"`
}

func (d *Deps) startQueue(w http.ResponseWriter, r *http.Request) {
	var req startQueueRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		jsonError(w, "invalid_request", http.StatusBadRequest)
		return
	}

	userID := auth.UserIDFromCtx(r.Context())
	deviceID := auth.DeviceIDFromCtx(r.Context())

	var items []queue.Item
	var err error
	switch req.SeedKind {
	case "song":
		items, err = d.buildSongRadio(r, req.SeedID)
	case "shuffle_liked":
		items, err = d.buildShuffleLiked(r, userID)
	default:
		jsonError(w, "invalid_seed_kind", http.StatusBadRequest)
		return
	}
	if errors.Is(err, errSeedUnavailable) {
		jsonError(w, "seed_unavailable", http.StatusBadGateway)
		return
	}
	if err != nil {
		d.Log.Error().Err(err).Str("seed_kind", req.SeedKind).Msg("build-queue")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}
	if len(items) == 0 {
		jsonError(w, "empty_queue", http.StatusUnprocessableEntity)
		return
	}

	sess, err := d.Queue.Create(r.Context(), queue.CreateParams{
		UserID:   userID,
		DeviceID: deviceID,
		SeedKind: req.SeedKind,
		SeedID:   req.SeedID,
		Title:    req.Title,
		Items:    items,
	})
	if err != nil {
		d.Log.Error().Err(err).Msg("create-queue")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}

	jsonOK(w, toQueueResponse(sess))
}

func (d *Deps) getQueue(w http.ResponseWriter, r *http.Request) {
	id, err := uuid.Parse(chi.URLParam(r, "id"))
	if err != nil {
		jsonError(w, "invalid_id", http.StatusBadRequest)
		return
	}
	userID := auth.UserIDFromCtx(r.Context())

	sess, err := d.Queue.Get(r.Context(), id, userID)
	if errors.Is(err, queue.ErrNotFound) {
		jsonError(w, "not_found", http.StatusNotFound)
		return
	}
	if err != nil {
		d.Log.Error().Err(err).Msg("get-queue")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}
	jsonOK(w, toQueueResponse(sess))
}

// errSeedUnavailable indicates the seed could not be expanded into a queue
// (e.g. YouTube unavailable). Mapped to 502 so the client can retry/fallback.
var errSeedUnavailable = errors.New("seed unavailable")

// buildSongRadio expands a YouTube song seed into a radio queue via /next.
func (d *Deps) buildSongRadio(r *http.Request, seedID string) ([]queue.Item, error) {
	if d.YT == nil {
		return nil, errSeedUnavailable
	}
	videoID := strings.TrimPrefix(seedID, "yt:")
	if videoID == "" {
		return nil, errSeedUnavailable
	}
	items, _, err := queue.ExpandRadio(r.Context(), d.YT, videoID, minQueueItems)
	if err != nil {
		d.Log.Warn().Err(err).Str("video_id", videoID).Msg("radio-expand")
		return nil, errSeedUnavailable
	}
	return items, nil
}

// buildShuffleLiked builds a shuffled queue from the user's liked songs.
func (d *Deps) buildShuffleLiked(r *http.Request, userID uuid.UUID) ([]queue.Item, error) {
	rows, err := gen.New(d.DB).ListLikedSongs(r.Context(), gen.ListLikedSongsParams{
		UserID:   pgtype.UUID{Bytes: userID, Valid: true},
		PageSize: 200,
	})
	if err != nil {
		return nil, err
	}
	liked := make([]queue.LikedSong, len(rows))
	for i, row := range rows {
		ls := queue.LikedSong{MediaID: row.MediaID, Title: row.Title}
		if row.DurationMs.Valid {
			ls.DurationMs = int(row.DurationMs.Int32)
		}
		liked[i] = ls
	}
	rng := rand.New(rand.NewSource(time.Now().UnixNano()))
	return queue.BuildAutomix(liked, rng), nil
}

func toQueueResponse(s queue.Session) queueResponse {
	items := make([]queueItemResponse, len(s.Items))
	for i, it := range s.Items {
		items[i] = queueItemResponse{
			MediaID:    it.MediaID,
			Title:      it.Title,
			Artists:    it.Artists,
			DurationMs: it.DurationMs,
		}
	}
	return queueResponse{
		QueueID:  s.ID.String(),
		SeedKind: s.SeedKind,
		Title:    s.Title,
		Version:  s.Version,
		Items:    items,
	}
}
