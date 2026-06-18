package api

import (
	"encoding/json"
	"net/http"

	"github.com/iceice666/sunflower/server/internal/auth"
	"github.com/iceice666/sunflower/server/internal/db/gen"
	"github.com/jackc/pgx/v5/pgtype"
)

// impressionsRequest is the POST /api/v1/impressions batch body. The client logs
// which recommended items were shown so the novelty/dedupe filters can suppress
// over-exposed items on the next build.
type impressionsRequest struct {
	Impressions []impressionEntry `json:"impressions"`
}

type impressionEntry struct {
	SectionID string `json:"section_id"`
	Source    string `json:"source"`
	SeedID    string `json:"seed_id"`
	MediaID   string `json:"media_id"`
	Position  int    `json:"position"`
}

// postImpressions records a batch of recommendation impressions.
//
// POST /api/v1/impressions
func (d *Deps) postImpressions(w http.ResponseWriter, r *http.Request) {
	userID := auth.UserIDFromCtx(r.Context())

	var req impressionsRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		jsonError(w, "invalid_request", http.StatusBadRequest)
		return
	}
	if len(req.Impressions) == 0 {
		w.WriteHeader(http.StatusNoContent)
		return
	}

	q := gen.New(d.DB)
	var written int
	for _, e := range req.Impressions {
		if e.MediaID == "" {
			continue
		}
		if err := q.InsertImpression(r.Context(), gen.InsertImpressionParams{
			UserID:    pgtype.UUID{Bytes: userID, Valid: true},
			SectionID: textParam(e.SectionID),
			Source:    textParam(e.Source),
			SeedID:    textParam(e.SeedID),
			MediaID:   textParam(e.MediaID),
			Position:  pgtype.Int4{Int32: int32(e.Position), Valid: true},
		}); err != nil {
			d.Log.Warn().Err(err).Msg("impressions: insert")
			continue
		}
		written++
	}
	jsonOK(w, map[string]int{"written": written})
}

// textParam wraps a string into a pgtype.Text, treating "" as NULL.
func textParam(s string) pgtype.Text {
	return pgtype.Text{String: s, Valid: s != ""}
}
