package recs

import (
	"context"
	"time"

	"github.com/google/uuid"
	"github.com/iceice666/sunflower/server/internal/db/gen"
	"github.com/iceice666/sunflower/server/internal/innertube/models"
	"github.com/jackc/pgx/v5/pgtype"
)

// recentWindow is the trailing window for "most played" aggregation.
const recentWindow = 90 * 24 * time.Hour

// staleThreshold is how long since last play before a heavy-rotation track
// counts as a "forgotten favorite".
const staleThreshold = 30 * 24 * time.Hour

// loadImpressions returns media_id → recent show count for novelty/dedupe. A nil
// map (no DB / error) is a valid "no impressions" result — every filter and
// scorer treats a missing key as unseen.
func (e *Engine) loadImpressions(ctx context.Context, userID uuid.UUID) map[string]int {
	out := map[string]int{}
	if e.db == nil {
		return out
	}
	since := pgtype.Timestamptz{Time: e.clock().Add(-24 * time.Hour), Valid: true}
	rows, err := gen.New(e.db).RecentImpressionMediaIDs(ctx, gen.RecentImpressionMediaIDsParams{
		UserID: pgtype.UUID{Bytes: userID, Valid: true},
		Since:  since,
	})
	if err != nil {
		return out
	}
	for _, r := range rows {
		if r.MediaID.Valid {
			out[r.MediaID.String] = int(r.Shows)
		}
	}
	return out
}

// mostPlayedCandidates loads the user's most-played songs in the recent window
// as candidates carrying their play-count and recency signals.
func (e *Engine) mostPlayedCandidates(ctx context.Context, userID uuid.UUID, limit int) []Candidate {
	if e.db == nil {
		return nil
	}
	rows, err := gen.New(e.db).MostPlayedSongs(ctx, gen.MostPlayedSongsParams{
		UserID:   pgtype.UUID{Bytes: userID, Valid: true},
		Since:    pgtype.Timestamptz{Time: e.clock().Add(-recentWindow), Valid: true},
		PageSize: int32(limit),
	})
	if err != nil {
		e.log.Warn().Err(err).Msg("recs: most-played query")
		return nil
	}
	out := make([]Candidate, 0, len(rows))
	for _, r := range rows {
		out = append(out, Candidate{
			MediaID:    r.SongMediaID,
			Title:      r.Title,
			Artists:    artistsOf(r.ArtistName),
			AlbumID:    textOr(r.AlbumID),
			DurationMs: int4Or(r.DurationMs),
			Source:     sourceOf(r.SourceType),
			Explicit:   r.Explicit,
			VideoOnly:  r.VideoOnly,
			PlayCount:  int(r.PlayCount),
			LastPlayed: timeOf(r.LastPlayedAt),
		})
	}
	return out
}

// forgottenCandidates loads heavy-rotation tracks not played recently.
func (e *Engine) forgottenCandidates(ctx context.Context, userID uuid.UUID, limit int) []Candidate {
	if e.db == nil {
		return nil
	}
	rows, err := gen.New(e.db).ForgottenFavorites(ctx, gen.ForgottenFavoritesParams{
		UserID:      pgtype.UUID{Bytes: userID, Valid: true},
		StaleBefore: pgtype.Timestamptz{Time: e.clock().Add(-staleThreshold), Valid: true},
		PageSize:    int32(limit),
	})
	if err != nil {
		return nil
	}
	out := make([]Candidate, 0, len(rows))
	for _, r := range rows {
		out = append(out, Candidate{
			MediaID:    r.SongMediaID,
			Title:      r.Title,
			Artists:    artistsOf(r.ArtistName),
			AlbumID:    textOr(r.AlbumID),
			DurationMs: int4Or(r.DurationMs),
			Source:     sourceOf(r.SourceType),
			PlayCount:  int(r.PlayCount),
			LastPlayed: timeOf(r.LastPlayedAt),
		})
	}
	return out
}

// songItemToCandidate converts an InnerTube SongItem into a remote candidate.
// remoteConfidence reflects how strongly the source vouched for it.
func songItemToCandidate(s models.SongItem, remoteConfidence float64) Candidate {
	if s.VideoID == "" {
		return Candidate{}
	}
	return Candidate{
		MediaID:          "yt:" + s.VideoID,
		Title:            s.Title,
		Artists:          s.Artists,
		DurationMs:       s.DurationMs,
		Source:           "yt",
		ThumbURL:         s.ThumbnailURL,
		RemoteConfidence: remoteConfidence,
	}
}

// --- small pgtype/format helpers ------------------------------------------

func artistsOf(name string) []string {
	if name == "" {
		return nil
	}
	return []string{name}
}

func textOr(t pgtype.Text) string {
	if t.Valid {
		return t.String
	}
	return ""
}

func int4Or(v pgtype.Int4) int {
	if v.Valid {
		return int(v.Int32)
	}
	return 0
}

// sourceOf maps a songs.source_type ("local"|"yt") to the affinity source key.
func sourceOf(sourceType string) string {
	if sourceType == "" {
		return "local"
	}
	return sourceType
}

// timeOf coerces the interface{} MAX(occurred_at) column (sqlc types aggregate
// results as interface{}) into a time.Time. Returns zero when absent.
func timeOf(v interface{}) time.Time {
	switch t := v.(type) {
	case time.Time:
		return t
	case pgtype.Timestamptz:
		if t.Valid {
			return t.Time
		}
	}
	return time.Time{}
}
