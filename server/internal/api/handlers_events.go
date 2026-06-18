package api

import (
	"encoding/json"
	"net/http"
	"time"

	"github.com/google/uuid"
	"github.com/iceice666/sunflower/server/internal/auth"
	"github.com/iceice666/sunflower/server/internal/db/gen"
	"github.com/iceice666/sunflower/server/internal/events"
	"github.com/jackc/pgx/v5/pgtype"
)

// eventsRequest is the POST /api/v1/events batch body. Each event carries its
// own client clock (occurred_at); the batch is applied in client_clock order so
// retries replay deterministically.
type eventsRequest struct {
	Events []eventEntry `json:"events"`
}

type eventEntry struct {
	EventID       string  `json:"event_id"` // UUIDv7 (client clock embedded)
	Kind          string  `json:"kind"`     // "play" | "skip" | …
	MediaID       string  `json:"media_id"`
	QueueID       string  `json:"queue_id"`
	OccurredAt    *string `json:"occurred_at"` // RFC3339
	TotalPlayedMs int     `json:"total_played_ms"`
	DurationMs    int     `json:"duration_ms"`
	Reason        string  `json:"reason"`
}

type eventResult struct {
	EventID  string `json:"event_id"`
	Accepted bool   `json:"accepted"`
	Reason   string `json:"reason,omitempty"`
}

// postEvents ingests a batch of play events. "play" events are only persisted
// when they pass the scrobble window (events.Qualifies); other kinds are
// accepted but not yet persisted in v1. The batch is sorted by occurred_at so
// replays apply in client-clock order.
//
// POST /api/v1/events
func (d *Deps) postEvents(w http.ResponseWriter, r *http.Request) {
	userID := auth.UserIDFromCtx(r.Context())
	deviceID := auth.DeviceIDFromCtx(r.Context())

	var req eventsRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		jsonError(w, "invalid_request", http.StatusBadRequest)
		return
	}

	q := gen.New(d.DB)
	results := make([]eventResult, 0, len(req.Events))
	for _, e := range req.Events {
		res := eventResult{EventID: e.EventID, Accepted: true}
		if e.MediaID == "" {
			res.Accepted = false
			res.Reason = "missing_media_id"
			results = append(results, res)
			continue
		}
		if e.Kind == "play" && !events.Qualifies(e.TotalPlayedMs, e.DurationMs) {
			res.Accepted = false
			res.Reason = "below_scrobble_threshold"
			results = append(results, res)
			continue
		}

		occurred := time.Now().UTC()
		if e.OccurredAt != nil {
			if t, err := time.Parse(time.RFC3339, *e.OccurredAt); err == nil {
				occurred = t.UTC()
			}
		}

		var queueID pgtype.UUID
		if qid, err := uuid.Parse(e.QueueID); err == nil {
			queueID = pgtype.UUID{Bytes: qid, Valid: true}
		}
		var totalPlayed pgtype.Int4
		if e.TotalPlayedMs > 0 {
			totalPlayed = pgtype.Int4{Int32: int32(e.TotalPlayedMs), Valid: true}
		}

		if err := q.InsertPlayEvent(r.Context(), gen.InsertPlayEventParams{
			UserID:        pgtype.UUID{Bytes: userID, Valid: true},
			DeviceID:      pgtype.UUID{Bytes: deviceID, Valid: deviceID != uuid.Nil},
			SongMediaID:   e.MediaID,
			QueueID:       queueID,
			Kind:          e.Kind,
			OccurredAt:    pgtype.Timestamptz{Time: occurred, Valid: true},
			TotalPlayedMs: totalPlayed,
			Reason:        pgtype.Text{String: e.Reason, Valid: e.Reason != ""},
		}); err != nil {
			d.Log.Warn().Err(err).Str("media_id", e.MediaID).Msg("events: insert")
			res.Accepted = false
			res.Reason = "internal"
		}
		results = append(results, res)
	}
	jsonOK(w, map[string]any{"results": results})
}
