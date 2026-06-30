package api_test

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/iceice666/sunflower/server/db/migrations"
	"github.com/iceice666/sunflower/server/internal/api"
	"github.com/iceice666/sunflower/server/internal/jobs"
	"github.com/iceice666/sunflower/server/internal/library"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/jackc/pgx/v5/stdlib"
	"github.com/pressly/goose/v3"
	"github.com/rs/zerolog"
	"github.com/testcontainers/testcontainers-go"
	tcpostgres "github.com/testcontainers/testcontainers-go/modules/postgres"
	"github.com/testcontainers/testcontainers-go/wait"
)

// TestM1Integration tests the full M1 flow:
//
//   - POST /api/v1/auth/register-device returns a token
//   - Unauthenticated requests return 401
//   - POST /api/v1/library/scan enqueues a job; after completion all tracks appear in DB
//   - GET /api/v1/library/songs returns flat JSON (no pgtype wrappers)
//
// Requires a running Docker / OrbStack daemon.
func TestM1Integration(t *testing.T) {
	ctx := context.Background()

	pgc, err := tcpostgres.Run(ctx,
		"postgres:16-alpine",
		tcpostgres.WithDatabase("sunflower"),
		tcpostgres.WithUsername("postgres"),
		tcpostgres.WithPassword("postgres"),
		testcontainers.WithWaitStrategy(
			wait.ForLog("database system is ready to accept connections").
				WithOccurrence(2),
		),
	)
	if err != nil {
		t.Fatalf("start postgres container: %v", err)
	}
	t.Cleanup(func() { _ = pgc.Terminate(ctx) })

	dsn, err := pgc.ConnectionString(ctx, "sslmode=disable")
	if err != nil {
		t.Fatalf("get connection string: %v", err)
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

	dataDir := t.TempDir()
	scanner := library.NewScanner(pool, dataDir, zerolog.Nop())
	jobReg := jobs.NewRegistry()
	handler := api.NewRouter(api.Deps{
		Log:     zerolog.Nop(),
		DB:      pool,
		Jobs:    jobReg,
		Scanner: scanner,
	})
	srv := httptest.NewServer(handler)
	t.Cleanup(srv.Close)

	// --- 1. Pair a device ---
	reg := pairTestDevice(t, srv, "test-device")
	if !strings.HasPrefix(reg.Token, "sf_dev_") {
		t.Errorf("token format: got %q, want prefix 'sf_dev_'", reg.Token)
	}

	// --- 2. Unauthenticated requests must return 401 ---
	r401, _ := http.Get(srv.URL + "/api/v1/library/songs")
	r401.Body.Close()
	if r401.StatusCode != http.StatusUnauthorized {
		t.Errorf("unauthenticated: want 401, got %d", r401.StatusCode)
	}

	// --- 3. Scan a directory with 3 synthesized MP3 files ---
	musicDir := t.TempDir()
	for i := 1; i <= 3; i++ {
		data := makeMP3("Track "+strconv.Itoa(i), "Artist One", "Album Alpha", i, 2024)
		if err := os.WriteFile(filepath.Join(musicDir, "t"+strconv.Itoa(i)+".mp3"), data, 0o644); err != nil {
			t.Fatal(err)
		}
	}

	scanResp := doJSON(t, srv, http.MethodPost, "/api/v1/library/scan",
		map[string]any{"roots": []string{musicDir}}, reg.Token)
	if scanResp.StatusCode != http.StatusOK {
		t.Fatalf("start-scan: want 200, got %d", scanResp.StatusCode)
	}
	var scan struct {
		JobID string `json:"job_id"`
	}
	mustDecode(t, scanResp.Body, &scan)

	// --- 4. Poll job until completed (10s timeout) ---
	deadline := time.Now().Add(10 * time.Second)
	var finalProcessed int
	for time.Now().Before(deadline) {
		jobResp := doJSON(t, srv, http.MethodGet, "/api/v1/jobs/"+scan.JobID, nil, reg.Token)
		var jobBody struct {
			Status         string `json:"status"`
			ProcessedFiles int    `json:"processed_files"`
		}
		mustDecode(t, jobResp.Body, &jobBody)
		switch jobBody.Status {
		case "completed":
			finalProcessed = jobBody.ProcessedFiles
			goto jobDone
		case "failed":
			t.Fatalf("scan job failed")
		}
		time.Sleep(50 * time.Millisecond)
	}
	t.Fatal("scan job did not complete within 10s")
jobDone:
	if finalProcessed != 3 {
		t.Errorf("processed_files: got %d, want 3", finalProcessed)
	}

	// --- 5. List songs — verify flat JSON shape and correct count ---
	songsResp := doJSON(t, srv, http.MethodGet, "/api/v1/library/songs", nil, reg.Token)
	if songsResp.StatusCode != http.StatusOK {
		t.Fatalf("list-songs: want 200, got %d", songsResp.StatusCode)
	}
	raw, _ := io.ReadAll(songsResp.Body)
	songsResp.Body.Close()

	// Decode into strict flat struct — if pgtype wrappers leak, this fails.
	// artist_name and album_title must be plain strings (not {"String":…,"Valid":…})
	// because COALESCE in the query prevents NULL from flowing through to pgtype.
	var songsBody struct {
		Songs []struct {
			MediaID    string `json:"media_id"`
			Title      string `json:"title"`
			SourceType string `json:"source_type"`
			ArtistName string `json:"artist_name"`
			AlbumTitle string `json:"album_title"`
			HasArt     bool   `json:"has_art"`
		} `json:"songs"`
	}
	if err := json.Unmarshal(raw, &songsBody); err != nil {
		t.Fatalf("decode songs: %v\nbody: %s", err, raw)
	}
	if len(songsBody.Songs) != 3 {
		t.Errorf("want 3 songs, got %d\nbody: %s", len(songsBody.Songs), raw)
	}
	for _, s := range songsBody.Songs {
		if s.MediaID == "" {
			t.Errorf("song has empty media_id: %+v", s)
		}
		if !strings.HasPrefix(s.MediaID, "local:") {
			t.Errorf("song media_id should start with 'local:': %q", s.MediaID)
		}
		if s.SourceType != "local" {
			t.Errorf("song source_type: got %q, want %q", s.SourceType, "local")
		}
		if s.ArtistName != "Artist One" {
			t.Errorf("song artist_name: got %q, want %q", s.ArtistName, "Artist One")
		}
		if s.AlbumTitle != "Album Alpha" {
			t.Errorf("song album_title: got %q, want %q", s.AlbumTitle, "Album Alpha")
		}
		// has_art is true whenever album_id IS NOT NULL — all fixture tracks have an
		// album, so the sql boolean expression (s.album_id IS NOT NULL) must be true.
		// This also exercises the interface{}→bool type assertion in toSongResponse.
		if !s.HasArt {
			t.Errorf("song %q: want has_art=true (has an album), got false", s.MediaID)
		}
	}
}

// TestUnauthenticatedReturns401 verifies middleware without a DB (nil pool safe
// because the 401 short-circuit fires before any DB call).
func TestUnauthenticatedReturns401(t *testing.T) {
	handler := api.NewRouter(api.Deps{Log: zerolog.Nop()})
	srv := httptest.NewServer(handler)
	defer srv.Close()

	for _, tc := range []struct {
		method string
		path   string
	}{
		{http.MethodGet, "/api/v1/library/songs"},
		{http.MethodGet, "/api/v1/library/albums"},
		{http.MethodGet, "/api/v1/library/artists"},
		{http.MethodPost, "/api/v1/library/scan"}, // POST route; wrong method returns 405, not 401
	} {
		req, _ := http.NewRequest(tc.method, srv.URL+tc.path, nil)
		resp, err := http.DefaultClient.Do(req)
		if err != nil {
			t.Fatalf("%s %s: %v", tc.method, tc.path, err)
		}
		resp.Body.Close()
		if resp.StatusCode != http.StatusUnauthorized {
			t.Errorf("%s %s: want 401, got %d", tc.method, tc.path, resp.StatusCode)
		}
	}
}

// TestM2StreamEndpoints verifies the M2 audio stream and album art routes.
//
// Requires Docker / OrbStack for the testcontainers Postgres instance.
func TestM2StreamEndpoints(t *testing.T) {
	ctx := context.Background()

	// --- Spin up Postgres (same pattern as TestM1Integration) ---
	pgc, err := tcpostgres.Run(ctx,
		"postgres:16-alpine",
		tcpostgres.WithDatabase("sunflower"),
		tcpostgres.WithUsername("postgres"),
		tcpostgres.WithPassword("postgres"),
		testcontainers.WithWaitStrategy(
			wait.ForLog("database system is ready to accept connections").
				WithOccurrence(2),
		),
	)
	if err != nil {
		t.Fatalf("start postgres container: %v", err)
	}
	t.Cleanup(func() { _ = pgc.Terminate(ctx) })

	dsn, err := pgc.ConnectionString(ctx, "sslmode=disable")
	if err != nil {
		t.Fatalf("get connection string: %v", err)
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

	dataDir := t.TempDir()
	scanner := library.NewScanner(pool, dataDir, zerolog.Nop())
	jobReg := jobs.NewRegistry()
	handler := api.NewRouter(api.Deps{
		Log:     zerolog.Nop(),
		DB:      pool,
		Jobs:    jobReg,
		Scanner: scanner,
		DataDir: dataDir,
	})
	srv := httptest.NewServer(handler)
	t.Cleanup(srv.Close)

	// --- Pair a device ---
	reg := pairTestDevice(t, srv, "test")

	// --- Write a known MP3 to a temp dir and scan it ---
	musicDir := t.TempDir()
	mp3Bytes := makeMP3("Stream Test Track", "Artist One", "Album Alpha", 1, 2024)
	mp3Path := filepath.Join(musicDir, "stream_test.mp3")
	if err := os.WriteFile(mp3Path, mp3Bytes, 0o644); err != nil {
		t.Fatal(err)
	}

	scanResp := doJSON(t, srv, http.MethodPost, "/api/v1/library/scan",
		map[string]any{"roots": []string{musicDir}}, reg.Token)
	if scanResp.StatusCode != http.StatusOK {
		t.Fatalf("start-scan: want 200, got %d", scanResp.StatusCode)
	}
	var scan struct {
		JobID string `json:"job_id"`
	}
	mustDecode(t, scanResp.Body, &scan)

	// Wait for scan to finish.
	deadline := time.Now().Add(10 * time.Second)
	for time.Now().Before(deadline) {
		jobResp := doJSON(t, srv, http.MethodGet, "/api/v1/jobs/"+scan.JobID, nil, reg.Token)
		var jobBody struct {
			Status string `json:"status"`
		}
		mustDecode(t, jobResp.Body, &jobBody)
		if jobBody.Status == "completed" {
			break
		}
		if jobBody.Status == "failed" {
			t.Fatal("scan job failed")
		}
		time.Sleep(50 * time.Millisecond)
	}

	// Get the media_id for the scanned track.
	songsResp := doJSON(t, srv, http.MethodGet, "/api/v1/library/songs", nil, reg.Token)
	if songsResp.StatusCode != http.StatusOK {
		t.Fatalf("list-songs: want 200, got %d", songsResp.StatusCode)
	}
	var songsBody struct {
		Songs []struct {
			MediaID string `json:"media_id"`
		} `json:"songs"`
	}
	mustDecode(t, songsResp.Body, &songsBody)
	if len(songsBody.Songs) != 1 {
		t.Fatalf("want 1 song, got %d", len(songsBody.Songs))
	}
	mediaID := songsBody.Songs[0].MediaID

	// --- 1. Stream endpoint returns 200 and the correct bytes ---
	streamURL := "/api/v1/library/songs/" + mediaID + "/stream"
	streamResp := doJSON(t, srv, http.MethodGet, streamURL, nil, reg.Token)
	if streamResp.StatusCode != http.StatusOK {
		t.Fatalf("stream: want 200, got %d", streamResp.StatusCode)
	}
	gotBytes, _ := io.ReadAll(streamResp.Body)
	streamResp.Body.Close()
	if !bytes.Equal(gotBytes, mp3Bytes) {
		t.Errorf("stream body mismatch: got %d bytes, want %d", len(gotBytes), len(mp3Bytes))
	}

	// --- 2. Range request → 206 with Content-Range ---
	rangeReq, _ := http.NewRequest(http.MethodGet, srv.URL+streamURL, nil)
	rangeReq.Header.Set("Authorization", "Bearer "+reg.Token)
	rangeEnd := strconv.Itoa(min(99, len(mp3Bytes)-1))
	rangeReq.Header.Set("Range", "bytes=0-"+rangeEnd)
	rangeResp, err := http.DefaultClient.Do(rangeReq)
	if err != nil {
		t.Fatalf("range request: %v", err)
	}
	rangeResp.Body.Close()
	if rangeResp.StatusCode != http.StatusPartialContent {
		t.Errorf("range: want 206, got %d", rangeResp.StatusCode)
	}
	if cr := rangeResp.Header.Get("Content-Range"); cr == "" {
		t.Error("range: missing Content-Range header")
	}

	// --- 3. Unauthenticated stream → 401 ---
	unauth, _ := http.Get(srv.URL + streamURL)
	unauth.Body.Close()
	if unauth.StatusCode != http.StatusUnauthorized {
		t.Errorf("unauthenticated stream: want 401, got %d", unauth.StatusCode)
	}

	// --- 4. Art route returns 404 for fixture without embedded cover art ---
	// makeMP3 emits no APIC frame, so the scanner writes no art file.
	// We use "local:000000000000" as a placeholder (any unknown album_media_id works).
	artResp := doJSON(t, srv, http.MethodGet,
		"/api/v1/library/albums/local%3A000000000000/art", nil, reg.Token)
	artResp.Body.Close()
	if artResp.StatusCode != http.StatusNotFound {
		t.Errorf("art (no cover): want 404, got %d", artResp.StatusCode)
	}
}

type pairedDevice struct {
	Token    string `json:"token"`
	DeviceID string `json:"device_id"`
}

func pairTestDevice(t *testing.T, srv *httptest.Server, name string) pairedDevice {
	t.Helper()
	const ownerPassword = "sunflower owner password"

	statusResp := doJSON(t, srv, http.MethodGet, "/api/v1/setup/status", nil, "")
	var status struct {
		Configured bool `json:"configured"`
	}
	mustDecode(t, statusResp.Body, &status)
	if !status.Configured {
		setupResp := doJSON(t, srv, http.MethodPost, "/api/v1/setup/owner", map[string]string{
			"setup_token":  "sunflower-test-setup-token",
			"display_name": "Owner",
			"password":     ownerPassword,
		}, "")
		if setupResp.StatusCode != http.StatusOK {
			t.Fatalf("setup owner: want 200, got %d", setupResp.StatusCode)
		}
		setupResp.Body.Close()
	}

	loginResp := doJSON(t, srv, http.MethodPost, "/api/v1/admin/auth/login",
		map[string]string{"password": ownerPassword}, "")
	if loginResp.StatusCode != http.StatusOK {
		t.Fatalf("admin login: want 200, got %d", loginResp.StatusCode)
	}
	var login struct {
		CSRFToken string `json:"csrf_token"`
	}
	mustDecode(t, loginResp.Body, &login)
	cookies := loginResp.Cookies()
	if login.CSRFToken == "" {
		t.Fatal("admin login: empty csrf token")
	}

	pairResp := doAdminJSON(t, srv, http.MethodPost, "/api/v1/admin/pairing-codes",
		map[string]any{"label": name, "ttl_seconds": 600}, cookies, login.CSRFToken)
	if pairResp.StatusCode != http.StatusOK {
		t.Fatalf("create pairing code: want 200, got %d", pairResp.StatusCode)
	}
	var pair struct {
		PairingCode string `json:"pairing_code"`
	}
	mustDecode(t, pairResp.Body, &pair)
	if pair.PairingCode == "" {
		t.Fatal("create pairing code: empty code")
	}

	regResp := doJSON(t, srv, http.MethodPost, "/api/v1/auth/register-device",
		map[string]string{
			"device_name":    name,
			"platform":       "test",
			"client_version": "0.0.1",
			"pairing_code":   pair.PairingCode,
		}, "")
	if regResp.StatusCode != http.StatusOK {
		t.Fatalf("register-device: want 200, got %d", regResp.StatusCode)
	}
	var reg pairedDevice
	mustDecode(t, regResp.Body, &reg)
	if reg.Token == "" || reg.DeviceID == "" {
		t.Fatalf("register-device: incomplete response: %+v", reg)
	}
	return reg
}

// doJSON sends a JSON request and returns the response.
func doJSON(t *testing.T, srv *httptest.Server, method, path string, body any, token string) *http.Response {
	t.Helper()
	var r io.Reader
	if body != nil {
		b, _ := json.Marshal(body)
		r = bytes.NewReader(b)
	}
	req, err := http.NewRequest(method, srv.URL+path, r)
	if err != nil {
		t.Fatal(err)
	}
	if body != nil {
		req.Header.Set("Content-Type", "application/json")
	}
	if token != "" {
		req.Header.Set("Authorization", "Bearer "+token)
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	return resp
}

func doAdminJSON(t *testing.T, srv *httptest.Server, method, path string, body any, cookies []*http.Cookie, csrf string) *http.Response {
	t.Helper()
	var r io.Reader
	if body != nil {
		b, _ := json.Marshal(body)
		r = bytes.NewReader(b)
	}
	req, err := http.NewRequest(method, srv.URL+path, r)
	if err != nil {
		t.Fatal(err)
	}
	if body != nil {
		req.Header.Set("Content-Type", "application/json")
	}
	if csrf != "" {
		req.Header.Set("X-CSRF-Token", csrf)
	}
	for _, c := range cookies {
		req.AddCookie(c)
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	return resp
}

// doJSONWithHeaders is doJSON plus extra request headers (e.g. Idempotency-Key).
func doJSONWithHeaders(t *testing.T, srv *httptest.Server, method, path string, body any, token string, headers map[string]string) *http.Response {
	t.Helper()
	var r io.Reader
	if body != nil {
		b, _ := json.Marshal(body)
		r = bytes.NewReader(b)
	}
	req, err := http.NewRequest(method, srv.URL+path, r)
	if err != nil {
		t.Fatal(err)
	}
	if body != nil {
		req.Header.Set("Content-Type", "application/json")
	}
	if token != "" {
		req.Header.Set("Authorization", "Bearer "+token)
	}
	for k, v := range headers {
		req.Header.Set(k, v)
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	return resp
}

// mustDecode decodes JSON from r into v, closing r when done.
func mustDecode(t *testing.T, r io.ReadCloser, v any) {
	t.Helper()
	defer r.Close()
	if err := json.NewDecoder(r).Decode(v); err != nil {
		t.Fatalf("JSON decode: %v", err)
	}
}

// makeMP3 builds a minimal ID3v2.3-tagged byte slice that dhowden/tag can parse.
// No MPEG audio frames are included.
func makeMP3(title, artist, album string, trackNum, year int) []byte {
	var frames bytes.Buffer

	writeFrame := func(id, text string) {
		data := append([]byte{0}, []byte(text)...) // encoding 0 = ISO-8859-1
		size := len(data)
		frames.WriteString(id)
		frames.Write([]byte{byte(size >> 24), byte(size >> 16), byte(size >> 8), byte(size)})
		frames.Write([]byte{0, 0}) // frame flags
		frames.Write(data)
	}

	if title != "" {
		writeFrame("TIT2", title)
	}
	if artist != "" {
		writeFrame("TPE1", artist)
	}
	if album != "" {
		writeFrame("TALB", album)
	}
	if trackNum > 0 {
		writeFrame("TRCK", strconv.Itoa(trackNum))
	}
	if year > 0 {
		// TYER is the year frame in ID3v2.3; TDRC is ID3v2.4-only
		writeFrame("TYER", strconv.Itoa(year))
	}

	body := frames.Bytes()
	tagSize := len(body)

	var out bytes.Buffer
	out.WriteString("ID3")
	out.Write([]byte{3, 0, 0}) // ID3v2.3, minor 0, no flags
	out.Write([]byte{          // syncsafe 28-bit tag size
		byte((tagSize >> 21) & 0x7F),
		byte((tagSize >> 14) & 0x7F),
		byte((tagSize >> 7) & 0x7F),
		byte(tagSize & 0x7F),
	})
	out.Write(body)
	return out.Bytes()
}
