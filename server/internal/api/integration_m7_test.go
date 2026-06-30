package api_test

import (
	"context"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"github.com/google/uuid"
	"github.com/iceice666/sunflower/server/db/migrations"
	"github.com/iceice666/sunflower/server/internal/api"
	syncpkg "github.com/iceice666/sunflower/server/internal/sync"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/jackc/pgx/v5/stdlib"
	"github.com/pressly/goose/v3"
	"github.com/rs/zerolog"
	"github.com/testcontainers/testcontainers-go"
	tcpostgres "github.com/testcontainers/testcontainers-go/modules/postgres"
	"github.com/testcontainers/testcontainers-go/wait"
)

// TestM7SyncIntegration exercises the M7 server flow:
//
//	POST /likes with Idempotency-Key       → applies once
//	replay same key                        → idempotent_replay, no double-apply
//	POST /events batch (scrobble window)    → qualifying plays persisted
//	GC removes expired idempotency rows
//
// Requires Docker for the testcontainers Postgres.
func TestM7SyncIntegration(t *testing.T) {
	ctx := context.Background()

	pgc, err := tcpostgres.Run(ctx, "postgres:16-alpine",
		tcpostgres.WithDatabase("sunflower"),
		tcpostgres.WithUsername("postgres"),
		tcpostgres.WithPassword("postgres"),
		testcontainers.WithWaitStrategy(
			wait.ForLog("database system is ready to accept connections").WithOccurrence(2),
		),
	)
	if err != nil {
		t.Fatalf("start postgres: %v", err)
	}
	t.Cleanup(func() { _ = pgc.Terminate(ctx) })

	dsn, err := pgc.ConnectionString(ctx, "sslmode=disable")
	if err != nil {
		t.Fatalf("dsn: %v", err)
	}
	cfg, err := pgxpool.ParseConfig(dsn)
	if err != nil {
		t.Fatalf("parse dsn: %v", err)
	}
	sqlDB := stdlib.OpenDB(*cfg.ConnConfig)
	defer sqlDB.Close()
	goose.SetBaseFS(migrations.Files)
	if err := goose.SetDialect("postgres"); err != nil {
		t.Fatal(err)
	}
	if err := goose.UpContext(ctx, sqlDB, "."); err != nil {
		t.Fatalf("migrations: %v", err)
	}

	pool, err := pgxpool.New(ctx, dsn)
	if err != nil {
		t.Fatalf("pgxpool: %v", err)
	}
	defer pool.Close()

	handler := api.NewRouter(api.Deps{Log: zerolog.Nop(), DB: pool})
	srv := httptest.NewServer(handler)
	t.Cleanup(srv.Close)

	dev := pairTestDevice(t, srv, "m7")

	var userID string
	if err := pool.QueryRow(ctx, `SELECT id FROM users LIMIT 1`).Scan(&userID); err != nil {
		t.Fatalf("user id: %v", err)
	}
	seedSong(t, ctx, pool, "yt:sync1", "Sync Song", "yt")

	key := uuid.NewString()

	// --- 1. First like with an Idempotency-Key applies it ---
	resp1 := doJSONWithHeaders(t, srv, http.MethodPost, "/api/v1/likes",
		map[string]any{"media_id": "yt:sync1", "liked": true}, dev.Token,
		map[string]string{"Idempotency-Key": key})
	if resp1.StatusCode != http.StatusOK {
		t.Fatalf("like 1: want 200, got %d", resp1.StatusCode)
	}
	resp1.Body.Close()

	// --- 2. Replay the SAME key → idempotent replay, no second apply ---
	resp2 := doJSONWithHeaders(t, srv, http.MethodPost, "/api/v1/likes",
		map[string]any{"media_id": "yt:sync1", "liked": true}, dev.Token,
		map[string]string{"Idempotency-Key": key})
	if resp2.StatusCode != http.StatusOK {
		t.Fatalf("like replay: want 200, got %d", resp2.StatusCode)
	}
	if resp2.Header.Get("Idempotent-Replay") != "true" {
		t.Fatalf("replay should set Idempotent-Replay header")
	}
	resp2.Body.Close()

	// Exactly one like row, one idempotency row.
	var likeCount, idemCount int
	if err := pool.QueryRow(ctx, `SELECT count(*) FROM likes WHERE song_media_id='yt:sync1'`).Scan(&likeCount); err != nil {
		t.Fatalf("like count: %v", err)
	}
	if likeCount != 1 {
		t.Fatalf("want 1 like after replay, got %d", likeCount)
	}
	if err := pool.QueryRow(ctx, `SELECT count(*) FROM idempotency_log`).Scan(&idemCount); err != nil {
		t.Fatalf("idem count: %v", err)
	}
	if idemCount != 1 {
		t.Fatalf("want 1 idempotency row, got %d", idemCount)
	}

	// --- 3. Events batch: a qualifying play persists, a short one does not ---
	now := time.Now().UTC().Format(time.RFC3339)
	evResp := doJSON(t, srv, http.MethodPost, "/api/v1/events",
		map[string]any{"events": []map[string]any{
			{"event_id": uuid.NewString(), "kind": "play", "media_id": "yt:sync1",
				"occurred_at": now, "total_played_ms": 60000, "duration_ms": 200000},
			{"event_id": uuid.NewString(), "kind": "play", "media_id": "yt:sync1",
				"occurred_at": now, "total_played_ms": 1000, "duration_ms": 200000},
		}}, dev.Token)
	if evResp.StatusCode != http.StatusOK {
		t.Fatalf("events: want 200, got %d", evResp.StatusCode)
	}
	evResp.Body.Close()
	var playCount int
	if err := pool.QueryRow(ctx, `SELECT count(*) FROM play_events WHERE song_media_id='yt:sync1'`).Scan(&playCount); err != nil {
		t.Fatalf("play count: %v", err)
	}
	if playCount != 1 {
		t.Fatalf("want 1 persisted play (qualifying only), got %d", playCount)
	}

	// --- 4. GC removes an expired idempotency row ---
	// Force the existing row to be expired, then run a GC pass.
	if _, err := pool.Exec(ctx,
		`UPDATE idempotency_log SET expires_at = now() - interval '1 hour'`); err != nil {
		t.Fatalf("expire idem row: %v", err)
	}
	syncpkg.RunGC(ctx, pool, zerolog.Nop())
	if err := pool.QueryRow(ctx, `SELECT count(*) FROM idempotency_log`).Scan(&idemCount); err != nil {
		t.Fatalf("idem count after gc: %v", err)
	}
	if idemCount != 0 {
		t.Fatalf("GC should remove expired rows, %d remain", idemCount)
	}
}
