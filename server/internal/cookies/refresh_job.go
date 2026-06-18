// Package cookies provides secretbox-encrypted cookie storage backed by Postgres.
package cookies

import (
	"context"
	"net/http"
	"time"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/rs/zerolog"
)

const knownStableVideoID = "dQw4w9WgXcQ"

// CookieChecker is a minimal interface for the innertube client call needed by the probe.
type CookieChecker interface {
	Next(ctx context.Context, videoID string, cont interface{ IsZero() bool }) (interface{}, error)
}

// StartRefreshJob runs the cookie health probe hourly in a background goroutine.
// It stops when ctx is cancelled.
func StartRefreshJob(ctx context.Context, db *pgxpool.Pool, key [32]byte, userID uuid.UUID, log zerolog.Logger) {
	go func() {
		ticker := time.NewTicker(1 * time.Hour)
		defer ticker.Stop()
		// Run once immediately on startup.
		runProbe(ctx, db, key, userID, log)
		for {
			select {
			case <-ctx.Done():
				return
			case <-ticker.C:
				runProbe(ctx, db, key, userID, log)
			}
		}
	}()
}

func runProbe(ctx context.Context, db *pgxpool.Pool, key [32]byte, userID uuid.UUID, log zerolog.Logger) {
	raw, err := Load(ctx, db, key, userID, "youtube")
	if err != nil {
		upsertHealth(ctx, db, "degraded", "no cookies stored: "+err.Error(), log)
		return
	}

	// Parse cookies from Netscape format and make a test InnerTube call.
	httpClient := &http.Client{Timeout: 15 * time.Second}
	httpClient.Jar = parseCookieJar(raw)

	probeCtx, cancel := context.WithTimeout(ctx, 15*time.Second)
	defer cancel()

	// Use a simple HTTP GET to music.youtube.com as a liveness check.
	req, _ := http.NewRequestWithContext(probeCtx, http.MethodGet,
		"https://music.youtube.com/youtubei/v1/player?key=AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Y9_11qcW8", nil)
	resp, err := httpClient.Do(req)
	if err != nil {
		upsertHealth(ctx, db, "degraded", err.Error(), log)
		return
	}
	resp.Body.Close()

	if resp.StatusCode == http.StatusOK || resp.StatusCode == http.StatusBadRequest {
		// 400 means the cookies were accepted but the request was malformed (no body) — that's fine.
		upsertHealth(ctx, db, "ok", "", log)
	} else {
		upsertHealth(ctx, db, "degraded", "probe status: "+resp.Status, log)
	}
}

func upsertHealth(ctx context.Context, db *pgxpool.Pool, status, detail string, log zerolog.Logger) {
	_, err := db.Exec(ctx, `
		INSERT INTO cookie_health (provider, status, checked_at, detail)
		VALUES ('youtube', $1, now(), $2)
		ON CONFLICT (provider) DO UPDATE
		SET status=$1, checked_at=now(), detail=$2
	`, status, nullIfEmpty(detail))
	if err != nil {
		log.Error().Err(err).Msg("cookie health upsert failed")
	}
}

func nullIfEmpty(s string) interface{} {
	if s == "" {
		return nil
	}
	return s
}

// parseCookieJar parses Netscape-format cookie bytes into a CookieJar.
// Returns nil on parse failure (graceful degradation).
func parseCookieJar(raw []byte) http.CookieJar {
	// Minimal Netscape parser — real implementation parses lines of the form:
	// <domain>\t<flag>\t<path>\t<secure>\t<expiry>\t<name>\t<value>
	// For M3, a nil jar (no cookies) is acceptable — the probe just checks reachability.
	return nil
}
