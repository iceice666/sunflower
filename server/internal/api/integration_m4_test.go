package api_test

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strconv"
	"testing"
	"time"

	"github.com/iceice666/sunflower/server/db/migrations"
	"github.com/iceice666/sunflower/server/internal/api"
	"github.com/iceice666/sunflower/server/internal/innertube/continuation"
	"github.com/iceice666/sunflower/server/internal/innertube/models"
	"github.com/iceice666/sunflower/server/internal/queue"
	"github.com/iceice666/sunflower/server/internal/streamproxy"
	"github.com/iceice666/sunflower/server/internal/streams"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/jackc/pgx/v5/stdlib"
	"github.com/pressly/goose/v3"
	"github.com/rs/zerolog"
	"github.com/testcontainers/testcontainers-go"
	tcpostgres "github.com/testcontainers/testcontainers-go/modules/postgres"
	"github.com/testcontainers/testcontainers-go/wait"
)

// fakeInnertube is a deterministic stand-in for the InnerTube client. It serves
// a radio page for /next and a soon-to-expire stream URL for /player so the M4
// flow can be exercised without network access.
type fakeInnertube struct {
	streamURL string
}

func (f *fakeInnertube) Next(_ context.Context, _ string, _ continuation.Cursor) (json.RawMessage, error) {
	// 12 related items so the materialized queue clears the ≥10 floor.
	items := ""
	for i := 0; i < 12; i++ {
		if i > 0 {
			items += ","
		}
		id := "vid" + string(rune('A'+i))
		items += `{"playlistPanelVideoRenderer":{"videoId":"` + id + `",` +
			`"title":{"runs":[{"text":"Song ` + id + `"}]}}}`
	}
	page := `{"contents":{"singleColumnMusicWatchNextResultsRenderer":{"tabbedRenderer":` +
		`{"watchNextTabbedResultsRenderer":{"tabs":[{"tabRenderer":{"content":` +
		`{"musicQueueRenderer":{"content":{"playlistPanelRenderer":{"contents":[` +
		items + `]}}}}}}]}}}}}`
	return json.RawMessage(page), nil
}

func (f *fakeInnertube) Player(_ context.Context, _ string) (models.PlayerResponse, error) {
	return models.PlayerResponse{
		Stream: models.StreamURL{URL: f.streamURL, Itag: 251, MimeType: "audio/webm"},
	}, nil
}

// TestM4QueueAndNextIntegration exercises the full M4 server flow:
//
//	POST /api/v1/queue/start (song seed) → ≥10 materialized items
//	GET  /api/v1/next                    → resolved current + lookahead
//	POST /api/v1/streams/resolve         → re-resolve with proxy fallback
//
// Requires a running Docker / OrbStack daemon for the testcontainers Postgres.
func TestM4QueueAndNextIntegration(t *testing.T) {
	ctx := context.Background()

	pgc, err := tcpostgres.Run(ctx,
		"postgres:16-alpine",
		tcpostgres.WithDatabase("sunflower"),
		tcpostgres.WithUsername("postgres"),
		tcpostgres.WithPassword("postgres"),
		testcontainers.WithWaitStrategy(
			wait.ForLog("database system is ready to accept connections").WithOccurrence(2),
		),
	)
	if err != nil {
		t.Fatalf("start postgres container: %v", err)
	}
	t.Cleanup(func() { _ = pgc.Terminate(ctx) })

	dsn, err := pgc.ConnectionString(ctx, "sslmode=disable")
	if err != nil {
		t.Fatalf("connection string: %v", err)
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

	// A stream URL that is already near expiry; resolve must surface it.
	expireSoon := time.Now().Add(30 * time.Second).Unix()
	fakeYT := &fakeInnertube{
		streamURL: "https://r1.googlevideo.com/videoplayback?expire=" +
			itoaInt64(expireSoon) + "&itag=251",
	}
	signer := streamproxy.NewSigner([]byte("integration-test-key-0123456789"), 15*time.Minute)
	resolver := &streams.Resolver{YT: fakeYT, Signer: signer, ProxyPath: "/api/v1/streams/proxy"}

	handler := api.NewRouter(api.Deps{
		Log:     zerolog.Nop(),
		DB:      pool,
		Queue:   queue.NewStore(pool),
		Streams: resolver,
		Proxy:   &streamproxy.Handler{Signer: signer, Client: http.DefaultClient, Log: zerolog.Nop()},
		YT:      fakeYT,
	})
	srv := httptest.NewServer(handler)
	t.Cleanup(srv.Close)

	// Register a device for auth.
	regResp := doJSON(t, srv, http.MethodPost, "/api/v1/auth/register-device",
		map[string]string{"device_name": "m4", "platform": "test", "client_version": "0.0.1"}, "")
	if regResp.StatusCode != http.StatusOK {
		t.Fatalf("register-device: %d", regResp.StatusCode)
	}
	var reg struct {
		Token string `json:"token"`
	}
	mustDecode(t, regResp.Body, &reg)

	// --- 1. Start a queue from a YouTube song seed ---
	startResp := doJSON(t, srv, http.MethodPost, "/api/v1/queue/start",
		map[string]string{"seed_kind": "song", "seed_id": "yt:seed123", "title": "Test Radio"}, reg.Token)
	if startResp.StatusCode != http.StatusOK {
		t.Fatalf("queue/start: want 200, got %d", startResp.StatusCode)
	}
	var startBody struct {
		QueueID string `json:"queue_id"`
		Items   []struct {
			MediaID string `json:"media_id"`
		} `json:"items"`
	}
	mustDecode(t, startResp.Body, &startBody)
	if startBody.QueueID == "" {
		t.Fatal("queue/start: empty queue_id")
	}
	if len(startBody.Items) < 10 {
		t.Fatalf("queue/start: got %d items, want ≥10", len(startBody.Items))
	}

	// --- 2. GET /next returns current (resolved) + lookahead ---
	nextResp := doJSON(t, srv, http.MethodGet,
		"/api/v1/next?queue_id="+startBody.QueueID+"&position=0", nil, reg.Token)
	if nextResp.StatusCode != http.StatusOK {
		t.Fatalf("next: want 200, got %d", nextResp.StatusCode)
	}
	var nextBody struct {
		Current *struct {
			MediaID   string  `json:"media_id"`
			Source    string  `json:"source"`
			StreamURL string  `json:"stream_url"`
			ExpiresAt *string `json:"stream_expires_at"`
		} `json:"current"`
		Lookahead []struct {
			MediaID string `json:"media_id"`
		} `json:"lookahead"`
	}
	mustDecode(t, nextResp.Body, &nextBody)
	if nextBody.Current == nil {
		t.Fatal("next: current is nil")
	}
	if nextBody.Current.Source != "youtube" {
		t.Fatalf("next: current source = %q, want youtube", nextBody.Current.Source)
	}
	if nextBody.Current.ExpiresAt == nil {
		t.Fatal("next: youtube current must have a non-null stream_expires_at")
	}
	if len(nextBody.Lookahead) < 5 {
		t.Fatalf("next: got %d lookahead, want ≥5", len(nextBody.Lookahead))
	}

	// --- 3. POST /streams/resolve with proxy fallback ---
	resolveResp := doJSON(t, srv, http.MethodPost, "/api/v1/streams/resolve",
		map[string]any{"media_id": nextBody.Current.MediaID, "proxy": true}, reg.Token)
	if resolveResp.StatusCode != http.StatusOK {
		t.Fatalf("streams/resolve: want 200, got %d", resolveResp.StatusCode)
	}
	var resolveBody struct {
		Source    string `json:"source"`
		StreamURL string `json:"stream_url"`
	}
	mustDecode(t, resolveResp.Body, &resolveBody)
	if resolveBody.Source != "proxy" {
		t.Fatalf("streams/resolve: source = %q, want proxy", resolveBody.Source)
	}

	// The proxy URL's token must verify back to the upstream googlevideo URL.
	const prefix = "/api/v1/streams/proxy?token="
	if len(resolveBody.StreamURL) <= len(prefix) || resolveBody.StreamURL[:len(prefix)] != prefix {
		t.Fatalf("streams/resolve: stream_url = %q, want proxy token url", resolveBody.StreamURL)
	}
	tok := resolveBody.StreamURL[len(prefix):]
	back, err := signer.Verify(tok)
	if err != nil || back != fakeYT.streamURL {
		t.Fatalf("proxy token round-trip failed: back=%q err=%v", back, err)
	}
}

func itoaInt64(n int64) string {
	return strconv.FormatInt(n, 10)
}
