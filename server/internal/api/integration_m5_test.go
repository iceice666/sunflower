package api_test

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"github.com/iceice666/sunflower/server/db/migrations"
	"github.com/iceice666/sunflower/server/internal/api"
	"github.com/iceice666/sunflower/server/internal/innertube/continuation"
	"github.com/iceice666/sunflower/server/internal/innertube/models"
	"github.com/iceice666/sunflower/server/internal/recs"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/jackc/pgx/v5/stdlib"
	"github.com/pressly/goose/v3"
	"github.com/rs/zerolog"
	"github.com/testcontainers/testcontainers-go"
	tcpostgres "github.com/testcontainers/testcontainers-go/modules/postgres"
	"github.com/testcontainers/testcontainers-go/wait"
)

// recsFakeYT serves related items for /next so Daily Discover can expand a liked
// seed, and empty payloads elsewhere (guest-degraded sections).
type recsFakeYT struct{}

func (recsFakeYT) Next(_ context.Context, _ string, _ continuation.Cursor) (json.RawMessage, error) {
	page := `{"contents":{"singleColumnMusicWatchNextResultsRenderer":{"tabbedRenderer":` +
		`{"watchNextTabbedResultsRenderer":{"tabs":[{"tabRenderer":{"content":` +
		`{"musicQueueRenderer":{"content":{"playlistPanelRenderer":{"contents":[` +
		`{"playlistPanelVideoRenderer":{"videoId":"rel1","title":{"runs":[{"text":"Related One"}]}}},` +
		`{"playlistPanelVideoRenderer":{"videoId":"rel2","title":{"runs":[{"text":"Related Two"}]}}}` +
		`]}}}}}}]}}}}}`
	return json.RawMessage(page), nil
}
func (recsFakeYT) Browse(_ context.Context, _ string, _ continuation.Cursor) (json.RawMessage, error) {
	return json.RawMessage(`{}`), nil
}
func (recsFakeYT) Search(_ context.Context, _ string) (json.RawMessage, error) {
	return json.RawMessage(`{}`), nil
}
func (recsFakeYT) Player(_ context.Context, _ string) (models.PlayerResponse, error) {
	return models.PlayerResponse{}, nil
}

// TestM5HomeIntegration exercises the M5 server flow end-to-end:
//
//	seed local library + play_events + a like
//	GET  /api/v1/home              → ≥1 section (Quick Picks from local plays)
//	GET  /api/v1/home (again)      → served from rec_cache (fresh)
//	POST /api/v1/likes             → like toggles
//	POST /api/v1/playlists + items → playlist CRUD
//	POST /api/v1/impressions       → impressions logged
//
// Requires Docker for the testcontainers Postgres.
func TestM5HomeIntegration(t *testing.T) {
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

	engine := recs.NewEngine(recs.Options{DB: pool, YT: recsFakeYT{}, Log: zerolog.Nop()})
	handler := api.NewRouter(api.Deps{
		Log:  zerolog.Nop(),
		DB:   pool,
		Recs: engine,
	})
	srv := httptest.NewServer(handler)
	t.Cleanup(srv.Close)

	// Register device → also creates the single user.
	regResp := doJSON(t, srv, http.MethodPost, "/api/v1/auth/register-device",
		map[string]string{"device_name": "m5", "platform": "test", "client_version": "0.0.1"}, "")
	if regResp.StatusCode != http.StatusOK {
		t.Fatalf("register-device: %d", regResp.StatusCode)
	}
	var reg struct {
		Token    string `json:"token"`
		DeviceID string `json:"device_id"`
	}
	mustDecode(t, regResp.Body, &reg)

	// Resolve the user id for direct seeding.
	var userID string
	if err := pool.QueryRow(ctx, `SELECT id FROM users LIMIT 1`).Scan(&userID); err != nil {
		t.Fatalf("user id: %v", err)
	}

	// Seed a local song + several play events so Quick Picks has candidates.
	seedSong(t, ctx, pool, "local:song1", "Local Hit", "local")
	seedSong(t, ctx, pool, "yt:liked1", "Liked Remote", "yt")
	for i := 0; i < 5; i++ {
		if _, err := pool.Exec(ctx,
			`INSERT INTO play_events (user_id, song_media_id, kind, occurred_at)
			 VALUES ($1,$2,'play',$3)`,
			userID, "local:song1", time.Now().Add(-time.Duration(i)*time.Hour)); err != nil {
			t.Fatalf("seed play_event: %v", err)
		}
	}

	// --- 1. GET /home → at least Quick Picks ---
	homeResp := doJSON(t, srv, http.MethodGet, "/api/v1/home", nil, reg.Token)
	if homeResp.StatusCode != http.StatusOK {
		t.Fatalf("home: want 200, got %d", homeResp.StatusCode)
	}
	var home struct {
		Sections []struct {
			Kind  string `json:"kind"`
			Items []struct {
				MediaID string `json:"media_id"`
			} `json:"items"`
		} `json:"sections"`
		Stale bool `json:"stale"`
	}
	mustDecode(t, homeResp.Body, &home)
	if len(home.Sections) == 0 {
		t.Fatal("home: expected at least one section (Quick Picks)")
	}
	foundQuickPicks := false
	for _, s := range home.Sections {
		if s.Kind == "quick_picks" {
			foundQuickPicks = true
			if len(s.Items) == 0 {
				t.Fatal("quick_picks section is empty despite seeded plays")
			}
		}
	}
	if !foundQuickPicks {
		t.Fatal("home: quick_picks section missing")
	}

	// --- 2. GET /home again → served fresh from cache ---
	homeResp2 := doJSON(t, srv, http.MethodGet, "/api/v1/home", nil, reg.Token)
	if homeResp2.StatusCode != http.StatusOK {
		t.Fatalf("home (2): want 200, got %d", homeResp2.StatusCode)
	}
	var home2 struct {
		Stale bool `json:"stale"`
	}
	mustDecode(t, homeResp2.Body, &home2)
	if home2.Stale {
		t.Fatal("home (2): expected fresh cache hit, got stale")
	}
	// Verify a rec_cache row exists.
	var cacheCount int
	if err := pool.QueryRow(ctx, `SELECT count(*) FROM rec_cache`).Scan(&cacheCount); err != nil {
		t.Fatalf("rec_cache count: %v", err)
	}
	if cacheCount == 0 {
		t.Fatal("home: rec_cache not populated")
	}

	// --- 3. POST /likes ---
	likeResp := doJSON(t, srv, http.MethodPost, "/api/v1/likes",
		map[string]any{"media_id": "yt:liked1", "liked": true}, reg.Token)
	if likeResp.StatusCode != http.StatusOK {
		t.Fatalf("likes: want 200, got %d", likeResp.StatusCode)
	}
	var likeCount int
	if err := pool.QueryRow(ctx, `SELECT count(*) FROM likes WHERE song_media_id='yt:liked1'`).Scan(&likeCount); err != nil {
		t.Fatalf("like count: %v", err)
	}
	if likeCount != 1 {
		t.Fatalf("likes: want 1 row, got %d", likeCount)
	}
	// Replaying the same like (idempotent) must not create a duplicate.
	_ = doJSON(t, srv, http.MethodPost, "/api/v1/likes",
		map[string]any{"media_id": "yt:liked1", "liked": true}, reg.Token)
	if err := pool.QueryRow(ctx, `SELECT count(*) FROM likes WHERE song_media_id='yt:liked1'`).Scan(&likeCount); err != nil {
		t.Fatalf("like count 2: %v", err)
	}
	if likeCount != 1 {
		t.Fatalf("likes replay: want 1 row, got %d", likeCount)
	}

	// --- 4. Playlist CRUD ---
	plResp := doJSON(t, srv, http.MethodPost, "/api/v1/playlists",
		map[string]string{"title": "My Mix"}, reg.Token)
	if plResp.StatusCode != http.StatusOK {
		t.Fatalf("create playlist: %d", plResp.StatusCode)
	}
	var pl struct {
		ID string `json:"id"`
	}
	mustDecode(t, plResp.Body, &pl)
	if pl.ID == "" {
		t.Fatal("create playlist: empty id")
	}
	addResp := doJSON(t, srv, http.MethodPost, "/api/v1/playlists/"+pl.ID+"/items",
		map[string]string{"media_id": "local:song1"}, reg.Token)
	if addResp.StatusCode != http.StatusNoContent {
		t.Fatalf("add item: want 204, got %d", addResp.StatusCode)
	}
	getResp := doJSON(t, srv, http.MethodGet, "/api/v1/playlists/"+pl.ID, nil, reg.Token)
	var got struct {
		Items []struct {
			MediaID string `json:"media_id"`
		} `json:"items"`
		Version int64 `json:"version"`
	}
	mustDecode(t, getResp.Body, &got)
	if len(got.Items) != 1 || got.Items[0].MediaID != "local:song1" {
		t.Fatalf("playlist items: want [local:song1], got %+v", got.Items)
	}
	if got.Version < 2 {
		t.Fatalf("playlist version should bump after add, got %d", got.Version)
	}

	// --- 5. POST /impressions ---
	imprResp := doJSON(t, srv, http.MethodPost, "/api/v1/impressions",
		map[string]any{"impressions": []map[string]any{
			{"section_id": "quick_picks", "media_id": "local:song1", "position": 0},
		}}, reg.Token)
	if imprResp.StatusCode != http.StatusOK {
		t.Fatalf("impressions: want 200, got %d", imprResp.StatusCode)
	}
	var imprCount int
	if err := pool.QueryRow(ctx, `SELECT count(*) FROM recommendation_impressions`).Scan(&imprCount); err != nil {
		t.Fatalf("impression count: %v", err)
	}
	if imprCount != 1 {
		t.Fatalf("impressions: want 1 row, got %d", imprCount)
	}
}

// seedSong inserts a minimal available song row for recs candidate generation.
func seedSong(t *testing.T, ctx context.Context, pool *pgxpool.Pool, mediaID, title, source string) {
	t.Helper()
	if _, err := pool.Exec(ctx,
		`INSERT INTO songs (media_id, source_type, title, available)
		 VALUES ($1,$2,$3,true) ON CONFLICT (media_id) DO NOTHING`,
		mediaID, source, title); err != nil {
		t.Fatalf("seed song %s: %v", mediaID, err)
	}
}
