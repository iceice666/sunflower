// Package cookies provides secretbox-encrypted cookie storage backed by Postgres.
package cookies

import (
	"bytes"
	"context"
	"encoding/json"
	"net/http"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/rs/zerolog"
)

// StartRefreshJob runs the cookie health probe hourly in a background goroutine.
// It stops when ctx is cancelled. Cookies come from the shared Provider, so the
// probe validates whatever source the live client uses (encrypted store or file).
func StartRefreshJob(ctx context.Context, db *pgxpool.Pool, p *Provider, log zerolog.Logger) {
	go func() {
		ticker := time.NewTicker(1 * time.Hour)
		defer ticker.Stop()
		// Run once immediately on startup.
		runProbe(ctx, db, p, log)
		for {
			select {
			case <-ctx.Done():
				return
			case <-ticker.C:
				runProbe(ctx, db, p, log)
			}
		}
	}()
}

func runProbe(ctx context.Context, db *pgxpool.Pool, p *Provider, log zerolog.Logger) {
	cks := p.Cookies()
	if len(cks) == 0 {
		upsertHealth(ctx, db, "unknown", "no cookies configured", log)
		return
	}

	probeCtx, cancel := context.WithTimeout(ctx, 15*time.Second)
	defer cancel()

	// WEB_REMIX is the only client that honours login cookies (ANDROID clients
	// ignore them and always return LOGIN_REQUIRED). A playable response with
	// the cookies attached confirms they are accepted; LOGIN_REQUIRED or
	// 401/403 means they are missing or expired.
	body, _ := json.Marshal(map[string]any{
		"context": map[string]any{
			"client": map[string]any{
				"clientName":    "WEB_REMIX",
				"clientVersion": "1.20230501.01.00",
				"hl":            "en",
				"gl":            "US",
			},
		},
		"videoId": "dQw4w9WgXcQ",
	})
	req, _ := http.NewRequestWithContext(probeCtx, http.MethodPost,
		"https://music.youtube.com/youtubei/v1/player?key=AIzaSyC9XL3ZjWddXya6X74dJoCTL-NKNELL6Cg",
		bytes.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	for _, ck := range cks {
		req.AddCookie(ck)
	}

	httpClient := &http.Client{Timeout: 15 * time.Second}
	resp, err := httpClient.Do(req)
	if err != nil {
		upsertHealth(ctx, db, "degraded", err.Error(), log)
		return
	}
	defer resp.Body.Close()

	if resp.StatusCode == http.StatusUnauthorized || resp.StatusCode == http.StatusForbidden {
		upsertHealth(ctx, db, "degraded", "cookies rejected: "+resp.Status, log)
		return
	}
	if resp.StatusCode != http.StatusOK {
		upsertHealth(ctx, db, "degraded", "probe status: "+resp.Status, log)
		return
	}

	var pr struct {
		PlayabilityStatus struct {
			Status string `json:"status"`
		} `json:"playabilityStatus"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&pr); err != nil {
		upsertHealth(ctx, db, "degraded", "decode player response: "+err.Error(), log)
		return
	}
	if pr.PlayabilityStatus.Status != "OK" {
		upsertHealth(ctx, db, "degraded", "playability: "+pr.PlayabilityStatus.Status, log)
		return
	}
	upsertHealth(ctx, db, "ok", "", log)
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
