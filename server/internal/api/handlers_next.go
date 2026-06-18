package api

import (
	"errors"
	"net/http"
	"strconv"

	"github.com/google/uuid"
	"github.com/iceice666/sunflower/server/internal/auth"
	"github.com/iceice666/sunflower/server/internal/queue"
	"github.com/iceice666/sunflower/server/internal/streams"
)

// lookaheadCount is the number of upcoming items returned alongside current.
// The acceptance criteria require 5–8 lookahead items.
const lookaheadCount = 8

// nextResponse is the GET /api/v1/next payload: the resolved current track plus
// a window of upcoming (unresolved) items the client pre-buffers via
// POST /streams/resolve.
type nextResponse struct {
	QueueID   string              `json:"queue_id"`
	Position  int                 `json:"position"`
	Current   *resolvedStream     `json:"current"`
	Lookahead []queueItemResponse `json:"lookahead"`
	HasMore   bool                `json:"has_more"`
}

// getNext returns the current track (resolved) plus lookahead items.
//
// GET /api/v1/next?queue_id=<uuid>&position=<n>
func (d *Deps) getNext(w http.ResponseWriter, r *http.Request) {
	queueID, err := uuid.Parse(r.URL.Query().Get("queue_id"))
	if err != nil {
		jsonError(w, "invalid_queue_id", http.StatusBadRequest)
		return
	}
	position := 0
	if v := r.URL.Query().Get("position"); v != "" {
		n, err := strconv.Atoi(v)
		if err != nil || n < 0 {
			jsonError(w, "invalid_position", http.StatusBadRequest)
			return
		}
		position = n
	}

	userID := auth.UserIDFromCtx(r.Context())
	sess, err := d.Queue.Get(r.Context(), queueID, userID)
	if errors.Is(err, queue.ErrNotFound) {
		jsonError(w, "not_found", http.StatusNotFound)
		return
	}
	if err != nil {
		d.Log.Error().Err(err).Msg("next-get-queue")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}

	if position >= len(sess.Items) {
		jsonError(w, "position_out_of_range", http.StatusNotFound)
		return
	}

	resp := nextResponse{
		QueueID:   sess.ID.String(),
		Position:  position,
		Lookahead: []queueItemResponse{},
	}

	// Resolve the current item to a playable stream.
	cur := sess.Items[position]
	resolved, err := d.Streams.Resolve(r.Context(), cur.MediaID, streams.Options{})
	switch {
	case errors.Is(err, streams.ErrUnavailable):
		jsonError(w, "current_unavailable", http.StatusGone)
		return
	case err != nil:
		d.Log.Error().Err(err).Str("media_id", cur.MediaID).Msg("next-resolve-current")
		jsonError(w, "resolve_failed", http.StatusBadGateway)
		return
	}
	rs := toResolvedStream(resolved, cur)
	resp.Current = &rs

	// Lookahead window (metadata only; the client resolves each as it buffers).
	end := position + 1 + lookaheadCount
	if end > len(sess.Items) {
		end = len(sess.Items)
	}
	for _, it := range sess.Items[position+1 : end] {
		resp.Lookahead = append(resp.Lookahead, queueItemResponse{
			MediaID:    it.MediaID,
			Title:      it.Title,
			Artists:    it.Artists,
			DurationMs: it.DurationMs,
		})
	}
	resp.HasMore = end < len(sess.Items)

	jsonOK(w, resp)
}
