package api_test

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"

	"github.com/iceice666/sunflower/server/db/migrations"
	"github.com/iceice666/sunflower/server/internal/api"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/jackc/pgx/v5/stdlib"
	"github.com/pressly/goose/v3"
	"github.com/rs/zerolog"
	"github.com/testcontainers/testcontainers-go"
	tcpostgres "github.com/testcontainers/testcontainers-go/modules/postgres"
	"github.com/testcontainers/testcontainers-go/wait"
)

// TestM6DownloadsIntegration exercises the M6 server flow:
//
//	POST   /devices/{id}/downloads          → register
//	GET    /devices/{id}/downloads          → lists it
//	GET    /library/songs/{id}/hash         → SHA-256 of a local file
//	DELETE /devices/{id}/downloads/{media}  → removes it
//	cross-device register → 403
//
// Requires Docker for the testcontainers Postgres.
func TestM6DownloadsIntegration(t *testing.T) {
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

	// Write a real local file so the hash endpoint has something to read.
	dir := t.TempDir()
	songPath := filepath.Join(dir, "song.mp3")
	content := []byte("fake-audio-bytes-for-hashing")
	if err := os.WriteFile(songPath, content, 0o644); err != nil {
		t.Fatalf("write song file: %v", err)
	}
	wantSum := sha256.Sum256(content)
	wantHex := hex.EncodeToString(wantSum[:])

	handler := api.NewRouter(api.Deps{Log: zerolog.Nop(), DB: pool})
	srv := httptest.NewServer(handler)
	t.Cleanup(srv.Close)

	// Pair device A.
	devA := pairTestDevice(t, srv, "A")

	// Seed a local song row pointing at the temp file.
	if _, err := pool.Exec(ctx,
		`INSERT INTO songs (media_id, source_type, title, available, local_path)
		 VALUES ('local:dl1','local','Downloadable',true,$1)`, songPath); err != nil {
		t.Fatalf("seed song: %v", err)
	}

	// --- 1. Register a download ---
	regResp := doJSON(t, srv, http.MethodPost, "/api/v1/devices/"+devA.DeviceID+"/downloads",
		map[string]any{"media_id": "local:dl1", "local_path": "/data/dl1.mp3", "bytes": len(content)}, devA.Token)
	if regResp.StatusCode != http.StatusNoContent {
		t.Fatalf("register download: want 204, got %d", regResp.StatusCode)
	}

	// --- 2. List downloads ---
	listResp := doJSON(t, srv, http.MethodGet, "/api/v1/devices/"+devA.DeviceID+"/downloads", nil, devA.Token)
	var list struct {
		Downloads []struct {
			MediaID string `json:"media_id"`
		} `json:"downloads"`
	}
	mustDecode(t, listResp.Body, &list)
	if len(list.Downloads) != 1 || list.Downloads[0].MediaID != "local:dl1" {
		t.Fatalf("list downloads: want [local:dl1], got %+v", list.Downloads)
	}

	// --- 3. Hash the local song ---
	hashResp := doJSON(t, srv, http.MethodGet, "/api/v1/library/songs/local:dl1/hash", nil, devA.Token)
	if hashResp.StatusCode != http.StatusOK {
		t.Fatalf("hash: want 200, got %d", hashResp.StatusCode)
	}
	var hash struct {
		SHA256 string `json:"sha256"`
		Bytes  int64  `json:"bytes"`
	}
	mustDecode(t, hashResp.Body, &hash)
	if hash.SHA256 != wantHex {
		t.Fatalf("hash mismatch: got %s want %s", hash.SHA256, wantHex)
	}
	if hash.Bytes != int64(len(content)) {
		t.Fatalf("hash bytes: got %d want %d", hash.Bytes, len(content))
	}

	// --- 4. Cross-device register is forbidden ---
	devB := pairTestDevice(t, srv, "B")
	// Device B tries to register under device A's id.
	forbidden := doJSON(t, srv, http.MethodPost, "/api/v1/devices/"+devA.DeviceID+"/downloads",
		map[string]any{"media_id": "local:dl1", "local_path": "/x", "bytes": 1}, devB.Token)
	if forbidden.StatusCode != http.StatusForbidden {
		t.Fatalf("cross-device register: want 403, got %d", forbidden.StatusCode)
	}

	// --- 5. Delete the download ---
	delResp := doJSON(t, srv, http.MethodDelete,
		"/api/v1/devices/"+devA.DeviceID+"/downloads/local:dl1", nil, devA.Token)
	if delResp.StatusCode != http.StatusNoContent {
		t.Fatalf("delete download: want 204, got %d", delResp.StatusCode)
	}
	listResp2 := doJSON(t, srv, http.MethodGet, "/api/v1/devices/"+devA.DeviceID+"/downloads", nil, devA.Token)
	var list2 struct {
		Downloads []struct {
			MediaID string `json:"media_id"`
		} `json:"downloads"`
	}
	mustDecode(t, listResp2.Body, &list2)
	if len(list2.Downloads) != 0 {
		t.Fatalf("after delete: want 0 downloads, got %d", len(list2.Downloads))
	}
}
