package api_test

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"testing"

	"github.com/iceice666/sunflower/server/db/migrations"
	"github.com/iceice666/sunflower/server/internal/api"
	"github.com/iceice666/sunflower/server/internal/innertube/continuation"
	"github.com/iceice666/sunflower/server/internal/innertube/models"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/jackc/pgx/v5/stdlib"
	"github.com/pressly/goose/v3"
	"github.com/rs/zerolog"
	"github.com/testcontainers/testcontainers-go"
	tcpostgres "github.com/testcontainers/testcontainers-go/modules/postgres"
	"github.com/testcontainers/testcontainers-go/wait"
)

type searchFakeYT struct {
	raw json.RawMessage
	err error
}

func (f searchFakeYT) Search(_ context.Context, _ string) (json.RawMessage, error) {
	if f.err != nil {
		return nil, f.err
	}
	return f.raw, nil
}

func (f searchFakeYT) Next(_ context.Context, _ string, _ continuation.Cursor) (json.RawMessage, error) {
	return json.RawMessage(`{}`), nil
}

func (f searchFakeYT) Player(_ context.Context, _ string) (models.PlayerResponse, error) {
	return models.PlayerResponse{}, nil
}

func TestSearchHandler(t *testing.T) {
	ctx := context.Background()
	pool := testPool(t, ctx)
	defer pool.Close()

	token := registerTestDevice(t, httptest.NewServer(api.NewRouter(api.Deps{
		Log: zerolog.Nop(),
		DB:  pool,
	})))

	t.Run("empty query", func(t *testing.T) {
		srv := httptest.NewServer(api.NewRouter(api.Deps{Log: zerolog.Nop(), DB: pool}))
		t.Cleanup(srv.Close)
		resp := doJSON(t, srv, http.MethodGet, "/api/v1/search?q=a", nil, token)
		defer resp.Body.Close()
		if resp.StatusCode != http.StatusBadRequest {
			t.Fatalf("search short query: want 400, got %d", resp.StatusCode)
		}
	})

	t.Run("missing yt client", func(t *testing.T) {
		srv := httptest.NewServer(api.NewRouter(api.Deps{Log: zerolog.Nop(), DB: pool}))
		t.Cleanup(srv.Close)
		resp := doJSON(t, srv, http.MethodGet, "/api/v1/search?q=radiohead", nil, token)
		defer resp.Body.Close()
		if resp.StatusCode != http.StatusServiceUnavailable {
			t.Fatalf("search missing yt: want 503, got %d", resp.StatusCode)
		}
	})

	t.Run("innertube error", func(t *testing.T) {
		srv := httptest.NewServer(api.NewRouter(api.Deps{
			Log: zerolog.Nop(),
			DB:  pool,
			YT:  searchFakeYT{err: errors.New("upstream down")},
		}))
		t.Cleanup(srv.Close)
		resp := doJSON(t, srv, http.MethodGet, "/api/v1/search?q=radiohead", nil, token)
		defer resp.Body.Close()
		if resp.StatusCode != http.StatusBadGateway {
			t.Fatalf("search upstream error: want 502, got %d", resp.StatusCode)
		}
	})

	t.Run("parsed success", func(t *testing.T) {
		raw, err := os.ReadFile("../innertube/parser/testdata/search_response.json")
		if err != nil {
			t.Fatalf("read fixture: %v", err)
		}
		srv := httptest.NewServer(api.NewRouter(api.Deps{
			Log: zerolog.Nop(),
			DB:  pool,
			YT:  searchFakeYT{raw: raw},
		}))
		t.Cleanup(srv.Close)
		resp := doJSON(t, srv, http.MethodGet, "/api/v1/search?q=rick&limit=1", nil, token)
		defer resp.Body.Close()
		if resp.StatusCode != http.StatusOK {
			t.Fatalf("search success: want 200, got %d", resp.StatusCode)
		}
		var body struct {
			Query string `json:"query"`
			Songs []struct {
				MediaID      string   `json:"media_id"`
				Source       string   `json:"source"`
				Title        string   `json:"title"`
				Artists      []string `json:"artists"`
				ThumbnailURL string   `json:"thumbnail_url"`
			} `json:"songs"`
		}
		mustDecode(t, resp.Body, &body)
		if body.Query != "rick" {
			t.Fatalf("query: got %q", body.Query)
		}
		if len(body.Songs) != 1 {
			t.Fatalf("songs: got %d, want 1", len(body.Songs))
		}
		if body.Songs[0].MediaID != "yt:dQw4w9WgXcQ" {
			t.Fatalf("media_id: got %q", body.Songs[0].MediaID)
		}
		if body.Songs[0].Source != "yt" || body.Songs[0].Title == "" || len(body.Songs[0].Artists) == 0 || body.Songs[0].ThumbnailURL == "" {
			t.Fatalf("unexpected song payload: %+v", body.Songs[0])
		}
	})
}

func testPool(t *testing.T, ctx context.Context) *pgxpool.Pool {
	t.Helper()
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
	t.Cleanup(func() { _ = sqlDB.Close() })
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
	return pool
}

func registerTestDevice(t *testing.T, srv *httptest.Server) string {
	t.Helper()
	t.Cleanup(srv.Close)
	regResp := doJSON(t, srv, http.MethodPost, "/api/v1/auth/register-device",
		map[string]string{"device_name": "search", "platform": "test", "client_version": "0.0.1"}, "")
	defer regResp.Body.Close()
	if regResp.StatusCode != http.StatusOK {
		t.Fatalf("register-device: want 200, got %d", regResp.StatusCode)
	}
	var reg struct {
		Token string `json:"token"`
	}
	mustDecode(t, regResp.Body, &reg)
	if reg.Token == "" {
		t.Fatal("register-device: empty token")
	}
	return reg.Token
}
