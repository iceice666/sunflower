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

// StartRefreshJob runs the cookie health probe hourly in a background goroutine.
// It stops when ctx is cancelled. The probe looks up the first registered user at
// each tick so it works correctly on fresh installs where no user exists at startup.
func StartRefreshJob(ctx context.Context, db *pgxpool.Pool, key [32]byte, log zerolog.Logger) {
	go func() {
		ticker := time.NewTicker(1 * time.Hour)
		defer ticker.Stop()
		// Run once immediately on startup.
		runProbe(ctx, db, key, log)
		for {
			select {
			case <-ctx.Done():
				return
			case <-ticker.C:
				runProbe(ctx, db, key, log)
			}
		}
	}()
}

func runProbe(ctx context.Context, db *pgxpool.Pool, key [32]byte, log zerolog.Logger) {
	// Look up the first registered user at each tick. On a fresh install there
	// may be no users yet; in that case skip silently.
	var userID uuid.UUID
	if err := db.QueryRow(ctx, `SELECT id FROM users LIMIT 1`).Scan(&userID); err != nil {
		log.Debug().Msg("cookie probe: no users registered yet, skipping")
		return
	}

	raw, err := Load(ctx, db, key, userID, "youtube")
	if err != nil {
		upsertHealth(ctx, db, "degraded", "no cookies stored: "+err.Error(), log)
		return
	}

	jar := parseCookieJar(raw)
	if jar == nil {
		// Cookie jar could not be parsed; we cannot validate the cookies.
		upsertHealth(ctx, db, "degraded", "cookie jar parse failed; cannot validate cookies", log)
		return
	}

	httpClient := &http.Client{Timeout: 15 * time.Second, Jar: jar}

	probeCtx, cancel := context.WithTimeout(ctx, 15*time.Second)
	defer cancel()

	req, _ := http.NewRequestWithContext(probeCtx, http.MethodGet,
		"https://music.youtube.com/youtubei/v1/player?key=AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Y9_11qcW8", nil)
	resp, err := httpClient.Do(req)
	if err != nil {
		upsertHealth(ctx, db, "degraded", err.Error(), log)
		return
	}
	resp.Body.Close()

	if resp.StatusCode == http.StatusOK {
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
// Returns nil when the input cannot be parsed.
func parseCookieJar(raw []byte) http.CookieJar {
	// Minimal Netscape parser — real implementation parses lines of the form:
	// <domain>\t<flag>\t<path>\t<secure>\t<expiry>\t<name>\t<value>
	// Not yet implemented; returns nil.
	_ = raw
	return nil
}
