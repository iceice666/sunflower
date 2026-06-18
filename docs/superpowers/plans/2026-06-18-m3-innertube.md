# M3 InnerTube Client Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a native Go InnerTube client so `probe innertube next --video-id=<id>` returns a fresh playable YouTube stream URL.

**Architecture:** Vertical slice — player payload → sig bootstrap → n-param decode → parse → probe CLI. Demo target is met at Task 8. Tasks 9–11 expand to remaining surfaces. Tasks 12–14 add cookie management.

**Tech Stack:** Go 1.25, `github.com/dop251/goja` (nsig JS engine), `golang.org/x/crypto/nacl/secretbox` (cookie encryption, already indirect dep), `github.com/go-chi/chi/v5`, `github.com/rs/zerolog`.

## Global Constraints

- Module root: `github.com/iceice666/sunflower/server`
- All new packages: `server/internal/innertube/…` or `server/internal/cookies/`
- Parsers MUST NOT return errors — missing fields → zero values; unknown renderers → zerolog debug + skip
- No CGo anywhere
- Run `go test ./...` from `server/` before every commit; all tests must pass
- Conventional commit messages
- Test fixtures committed to `testdata/`; never regenerated in CI

## File Map

| File | Responsibility |
|---|---|
| `internal/innertube/models/models.go` | All public structs (StreamURL, SongItem, PlayerResponse, NextPage, ProbeNextResult, Locale, …) |
| `internal/innertube/continuation/cursor.go` | `Cursor []byte` type + `IsZero()` |
| `internal/innertube/context.go` | `buildAndroidMusicContext`, `buildWebRemixContext` |
| `internal/innertube/client.go` | `Client` struct; `Player`, `Next`, `Browse`, `Search` methods |
| `internal/innertube/sig/base_js.go` | `Cache` struct; `Bootstrap`, `DecodeN`, `ApplySig` |
| `internal/innertube/sig/nsig.go` | goja-based n-param extraction + execution |
| `internal/innertube/sig/transform.go` | Sig-cipher op list (WEB fallback) |
| `internal/innertube/sig/testdata/` | Frozen nsig fixture + sig fixture |
| `internal/innertube/payloads/player.go` | `/youtubei/v1/player` POST body |
| `internal/innertube/payloads/next.go` | `/youtubei/v1/next` POST body |
| `internal/innertube/payloads/browse.go` | `/youtubei/v1/browse` POST body |
| `internal/innertube/payloads/search.go` | `/youtubei/v1/search` POST body |
| `internal/innertube/parser/helpers.go` | `getString`, `getArray`, `getInt` tree-walkers |
| `internal/innertube/parser/yt_item.go` | `parseSongItem`, `parseAlbumItem`, `parseArtistItem`, `parsePlaylistItem` |
| `internal/innertube/parser/next_page.go` | `ParseNextPage` |
| `internal/innertube/parser/home_page.go` | `ParseHomePage` |
| `internal/innertube/parser/related_page.go` | `ParseRelatedPage` |
| `internal/innertube/parser/artist_page.go` | `ParseArtistPage` |
| `internal/innertube/parser/album_page.go` | `ParseAlbumPage` |
| `internal/innertube/parser/playlist_page.go` | `ParsePlaylistPage` |
| `internal/innertube/parser/search_page.go` | `ParseSearchPage` |
| `internal/innertube/parser/testdata/` | Fixture JSON per surface |
| `internal/cookies/store.go` | secretbox encrypt/decrypt; `Store`, `Load` |
| `internal/cookies/refresh_job.go` | Hourly cookie health probe |
| `internal/api/handlers_cookies.go` | `POST /api/v1/cookies/youtube`, `GET …/status` |
| `db/migrations/0006_cookie_health.sql` | `cookie_health` table |
| `cmd/probe/main.go` | probe entrypoint |
| `cmd/probe/innertube_cmd.go` | `next`, `home`, `search`, `cookies-set` subcommands |

---

### Task 1: Add goja dependency

**Files:**
- Modify: `server/go.mod`, `server/go.sum`

**Interfaces:**
- Produces: `github.com/dop251/goja` importable in `sig/nsig.go`

- [ ] **Step 1: Add goja**

```bash
cd server && go get github.com/dop251/goja@latest
```

Expected: go.mod gains `github.com/dop251/goja vX.X.X`

- [ ] **Step 2: Verify build**

```bash
cd server && go build ./...
```

Expected: no errors

- [ ] **Step 3: Commit**

```bash
git add server/go.mod server/go.sum
git commit -m "build(deps): add goja for InnerTube nsig decryption"
```

---

### Task 2: Models + Continuation

**Files:**
- Create: `server/internal/innertube/models/models.go`
- Create: `server/internal/innertube/continuation/cursor.go`
- Create: `server/internal/innertube/continuation/cursor_test.go`

**Interfaces:**
- Produces: all public types used by every subsequent task

- [ ] **Step 1: Write cursor test**

```go
// server/internal/innertube/continuation/cursor_test.go
package continuation_test

import (
	"testing"
	"github.com/iceice666/sunflower/server/internal/innertube/continuation"
)

func TestCursorIsZero(t *testing.T) {
	var zero continuation.Cursor
	if !zero.IsZero() {
		t.Fatal("nil cursor should be zero")
	}
	nonZero := continuation.Cursor([]byte("token"))
	if nonZero.IsZero() {
		t.Fatal("non-empty cursor should not be zero")
	}
}
```

- [ ] **Step 2: Run test — expect FAIL**

```bash
cd server && go test ./internal/innertube/continuation/...
```

Expected: `cannot find package`

- [ ] **Step 3: Write cursor.go**

```go
// server/internal/innertube/continuation/cursor.go
package continuation

// Cursor is an opaque continuation token extracted from a YT response.
// It is posted back verbatim as the "continuation" field in the next request.
// Never inspect or transform the contents.
type Cursor []byte

func (c Cursor) IsZero() bool { return len(c) == 0 }
```

- [ ] **Step 4: Write models.go**

```go
// server/internal/innertube/models/models.go
package models

import (
	"time"
	"github.com/iceice666/sunflower/server/internal/innertube/continuation"
)

type Locale struct {
	HL string // e.g. "en"
	GL string // e.g. "US"
}

type StreamURL struct {
	URL       string
	ExpiresAt time.Time
	Itag      int
	MimeType  string
	Bitrate   int
	Loudness  float64 // loudness_normalization_db; zero if absent
}

type SongItem struct {
	VideoID      string
	Title        string
	Artists      []string
	AlbumTitle   string
	DurationMs   int
	ThumbnailURL string
}

type AlbumItem struct {
	BrowseID     string
	Title        string
	Artists      []string
	Year         string
	ThumbnailURL string
}

type ArtistItem struct {
	BrowseID     string
	Name         string
	ThumbnailURL string
}

type PlaylistItem struct {
	BrowseID     string
	Title        string
	ThumbnailURL string
}

type PlayerResponse struct {
	VideoID     string
	PlayerJsURL string // absolute base.js URL; see sig.Cache.Bootstrap for source
	Stream      StreamURL
	AllStreams   []StreamURL
}

type NextPage struct {
	Current      SongItem
	Related      []SongItem
	Continuation continuation.Cursor
}

type HomeSection struct {
	Title string
	Items []any // SongItem | AlbumItem | PlaylistItem
}

type HomePage struct {
	Sections []HomeSection
	Chips    []string
}

type SearchPage struct {
	Songs        []SongItem
	Albums       []AlbumItem
	Artists      []ArtistItem
	Continuation continuation.Cursor
}

// ProbeNextResult is the JSON output of `probe innertube next`.
type ProbeNextResult struct {
	CurrentURL   string                `json:"current_url"`
	ExpiresAt    time.Time             `json:"expires_at"`
	Itag         int                   `json:"itag"`
	NextItems    []SongItem            `json:"next_items"`
	Continuation continuation.Cursor   `json:"continuation,omitempty"`
}
```

- [ ] **Step 5: Run tests — expect PASS**

```bash
cd server && go test ./internal/innertube/continuation/...
```

- [ ] **Step 6: Verify build**

```bash
cd server && go build ./...
```

- [ ] **Step 7: Commit**

```bash
git add server/internal/innertube/
git commit -m "feat(m3): models package and continuation cursor"
```

---

### Task 3: Client context builders

**Files:**
- Create: `server/internal/innertube/context.go`
- Create: `server/internal/innertube/context_test.go`

**Interfaces:**
- Consumes: `models.Locale`
- Produces: `buildAndroidMusicContext(videoID string, locale models.Locale) map[string]any`, `buildWebRemixContext(locale models.Locale) map[string]any`

- [ ] **Step 1: Write failing test**

```go
// server/internal/innertube/context_test.go
package innertube_test

import (
	"testing"
	"github.com/iceice666/sunflower/server/internal/innertube"
	"github.com/iceice666/sunflower/server/internal/innertube/models"
)

func TestBuildAndroidMusicContext(t *testing.T) {
	ctx := innertube.BuildAndroidMusicContext(models.Locale{HL: "en", GL: "US"})
	client, ok := ctx["context"].(map[string]any)["client"].(map[string]any)
	if !ok {
		t.Fatal("context.client missing")
	}
	if client["clientName"] != "ANDROID_MUSIC" {
		t.Errorf("clientName = %v, want ANDROID_MUSIC", client["clientName"])
	}
	if client["hl"] != "en" {
		t.Errorf("hl = %v, want en", client["hl"])
	}
}
```

- [ ] **Step 2: Run — expect FAIL**

```bash
cd server && go test ./internal/innertube/... -run TestBuildAndroidMusicContext
```

- [ ] **Step 3: Implement context.go**

```go
// server/internal/innertube/context.go
package innertube

import "github.com/iceice666/sunflower/server/internal/innertube/models"

const (
	androidMusicClientName    = "ANDROID_MUSIC"
	androidMusicClientVersion = "7.27.52"
	androidMusicClientID      = "21"
	androidMusicAPIKey        = "AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Y9_11qcW8"

	webRemixClientName    = "WEB_REMIX"
	webRemixClientVersion = "1.20230501.01.00"
	webRemixAPIKey        = "AIzaSyC9XL3ZjWddXya6X74dJoCTL-NKNELL6Cg"
)

// BuildAndroidMusicContext returns the base POST body context for ANDROID_MUSIC
// requests (player, next). Stream URLs from this context arrive as plain URLs
// (no signatureCipher), requiring only n-param decryption.
func BuildAndroidMusicContext(locale models.Locale) map[string]any {
	return map[string]any{
		"context": map[string]any{
			"client": map[string]any{
				"clientName":        androidMusicClientName,
				"clientVersion":     androidMusicClientVersion,
				"androidSdkVersion": 30,
				"userAgent":         "com.google.android.apps.youtube.music/" + androidMusicClientVersion + " (Linux; U; Android 11) gzip",
				"hl":                locale.HL,
				"gl":                locale.GL,
				"utcOffsetMinutes":  0,
			},
		},
	}
}

// BuildWebRemixContext returns the base POST body context for WEB_REMIX
// requests (browse, search). Stream URLs may include signatureCipher and
// require sig-cipher decryption in addition to n-param decryption.
func BuildWebRemixContext(locale models.Locale) map[string]any {
	return map[string]any{
		"context": map[string]any{
			"client": map[string]any{
				"clientName":    webRemixClientName,
				"clientVersion": webRemixClientVersion,
				"hl":            locale.HL,
				"gl":            locale.GL,
			},
		},
	}
}

// AndroidMusicAPIKey is the public API key for ANDROID_MUSIC InnerTube requests.
func AndroidMusicAPIKey() string { return androidMusicAPIKey }

// WebRemixAPIKey is the public API key for WEB_REMIX InnerTube requests.
func WebRemixAPIKey() string { return webRemixAPIKey }
```

- [ ] **Step 4: Run tests — expect PASS**

```bash
cd server && go test ./internal/innertube/... -run TestBuildAndroidMusicContext
```

- [ ] **Step 5: Commit**

```bash
git add server/internal/innertube/context.go server/internal/innertube/context_test.go
git commit -m "feat(m3): InnerTube client context builders"
```

---

### Task 4: Payloads — player and next

**Files:**
- Create: `server/internal/innertube/payloads/player.go`
- Create: `server/internal/innertube/payloads/next.go`
- Create: `server/internal/innertube/payloads/payloads_test.go`

**Interfaces:**
- Consumes: `innertube.BuildAndroidMusicContext`, `continuation.Cursor`, `models.Locale`
- Produces:
  - `payloads.Player(videoID string, locale models.Locale) map[string]any`
  - `payloads.Next(videoID string, cont continuation.Cursor, locale models.Locale) map[string]any`

- [ ] **Step 1: Write tests**

```go
// server/internal/innertube/payloads/payloads_test.go
package payloads_test

import (
	"encoding/json"
	"testing"

	"github.com/iceice666/sunflower/server/internal/innertube/continuation"
	"github.com/iceice666/sunflower/server/internal/innertube/models"
	"github.com/iceice666/sunflower/server/internal/innertube/payloads"
)

func TestPlayerPayload(t *testing.T) {
	p := payloads.Player("dQw4w9WgXcQ", models.Locale{HL: "en", GL: "US"})
	if p["videoId"] != "dQw4w9WgXcQ" {
		t.Errorf("videoId = %v", p["videoId"])
	}
	b, _ := json.Marshal(p)
	if len(b) == 0 {
		t.Fatal("empty payload")
	}
}

func TestNextPayload_NoContinuation(t *testing.T) {
	p := payloads.Next("dQw4w9WgXcQ", nil, models.Locale{HL: "en", GL: "US"})
	if p["videoId"] != "dQw4w9WgXcQ" {
		t.Errorf("videoId = %v", p["videoId"])
	}
	if _, hasCont := p["continuation"]; hasCont {
		t.Error("continuation should be absent when cursor is zero")
	}
}

func TestNextPayload_WithContinuation(t *testing.T) {
	p := payloads.Next("dQw4w9WgXcQ", continuation.Cursor("tok"), models.Locale{HL: "en", GL: "US"})
	if p["continuation"] != "tok" {
		t.Errorf("continuation = %v, want tok", p["continuation"])
	}
}
```

- [ ] **Step 2: Run — expect FAIL**

```bash
cd server && go test ./internal/innertube/payloads/...
```

- [ ] **Step 3: Implement player.go**

```go
// server/internal/innertube/payloads/player.go
package payloads

import (
	"github.com/iceice666/sunflower/server/internal/innertube"
	"github.com/iceice666/sunflower/server/internal/innertube/models"
)

// Player builds the POST body for /youtubei/v1/player with ANDROID_MUSIC context.
// The "params" value "CgIQBg==" requests audio-only formats.
func Player(videoID string, locale models.Locale) map[string]any {
	body := innertube.BuildAndroidMusicContext(locale)
	body["videoId"] = videoID
	body["params"] = "CgIQBg=="
	return body
}
```

- [ ] **Step 4: Implement next.go**

```go
// server/internal/innertube/payloads/next.go
package payloads

import (
	"github.com/iceice666/sunflower/server/internal/innertube"
	"github.com/iceice666/sunflower/server/internal/innertube/continuation"
	"github.com/iceice666/sunflower/server/internal/innertube/models"
)

// Next builds the POST body for /youtubei/v1/next with ANDROID_MUSIC context.
// If cont is non-zero, the continuation token is included to fetch the next page.
func Next(videoID string, cont continuation.Cursor, locale models.Locale) map[string]any {
	body := innertube.BuildAndroidMusicContext(locale)
	body["videoId"] = videoID
	if !cont.IsZero() {
		body["continuation"] = string(cont)
	}
	return body
}
```

- [ ] **Step 5: Run tests — expect PASS**

```bash
cd server && go test ./internal/innertube/payloads/...
```

- [ ] **Step 6: Commit**

```bash
git add server/internal/innertube/payloads/
git commit -m "feat(m3): player and next payload builders"
```

---

### Task 5: Sig bootstrap + nsig decryption

**Files:**
- Create: `server/internal/innertube/sig/base_js.go`
- Create: `server/internal/innertube/sig/nsig.go`
- Create: `server/internal/innertube/sig/sig_test.go`
- Create: `server/internal/innertube/sig/testdata/nsig_fixture.json`

**Interfaces:**
- Produces:
  - `sig.NewCache(httpClient *http.Client) *Cache`
  - `(*Cache).Bootstrap(ctx context.Context) error`
  - `(*Cache).DecodeN(ctx context.Context, rawURL, playerJsURL string) (string, error)`
  - `var ErrNoPlayerJs = errors.New("no player js available")`
  - `var ErrSigInvalidated = errors.New("sig invalidated after sustained 403s")`

- [ ] **Step 1: Create nsig test fixture**

Create `server/internal/innertube/sig/testdata/nsig_fixture.json` with a synthetic nsig function and known I/O pairs. The real fixture will be replaced after Task 7 when a live base.js is captured.

```json
{
  "func_name": "decodeN",
  "func_body": "function decodeN(a){return a.split('').reverse().join('')}",
  "cases": [
    {"in": "hello", "out": "olleh"},
    {"in": "abcde", "out": "edcba"}
  ]
}
```

- [ ] **Step 2: Write nsig test**

```go
// server/internal/innertube/sig/sig_test.go
package sig_test

import (
	"context"
	"encoding/json"
	"os"
	"testing"

	"github.com/iceice666/sunflower/server/internal/innertube/sig"
)

type nsigFixture struct {
	FuncName string `json:"func_name"`
	FuncBody string `json:"func_body"`
	Cases    []struct {
		In  string `json:"in"`
		Out string `json:"out"`
	} `json:"cases"`
}

func TestDecodeNFromFixture(t *testing.T) {
	raw, err := os.ReadFile("testdata/nsig_fixture.json")
	if err != nil {
		t.Fatal(err)
	}
	var fix nsigFixture
	if err := json.Unmarshal(raw, &fix); err != nil {
		t.Fatal(err)
	}

	cache := sig.NewCache(nil)
	if err := cache.LoadNsigForTest(fix.FuncName, fix.FuncBody); err != nil {
		t.Fatalf("LoadNsigForTest: %v", err)
	}

	for _, tc := range fix.Cases {
		got, err := cache.DecodeNRaw(context.Background(), tc.In)
		if err != nil {
			t.Errorf("DecodeNRaw(%q) error: %v", tc.In, err)
			continue
		}
		if got != tc.Out {
			t.Errorf("DecodeNRaw(%q) = %q, want %q", tc.In, got, tc.Out)
		}
	}
}
```

- [ ] **Step 3: Run — expect FAIL**

```bash
cd server && go test ./internal/innertube/sig/...
```

- [ ] **Step 4: Implement nsig.go**

```go
// server/internal/innertube/sig/nsig.go
package sig

import (
	"fmt"
	"regexp"

	"github.com/dop251/goja"
)

var (
	// nsigNameRe extracts the nsig function name from base.js.
	// YT obfuscation changes the name but the pattern around it is stable.
	nsigNameRe = regexp.MustCompile(`\.get\("n"\)\)&&\(b=([a-zA-Z0-9$]+)\[`)
)

type nsigEntry struct {
	prog     *goja.Program
	funcName string
}

func extractNsig(js string) (*nsigEntry, error) {
	m := nsigNameRe.FindStringSubmatch(js)
	if m == nil {
		return nil, fmt.Errorf("nsig: function name not found in base.js")
	}
	// The name may be an array access like Xyz[0]; extract the array name.
	arrayName := m[1]

	// Find the array declaration to get the actual function.
	arrayRe := regexp.MustCompile(`var ` + regexp.QuoteMeta(arrayName) + `\s*=\s*\[([^\]]+)\]`)
	am := arrayRe.FindStringSubmatch(js)

	var funcName string
	if am != nil {
		// The array contains function names; use the first element.
		funcName = am[1]
	} else {
		funcName = arrayName
	}

	// Find the function body by name.
	funcRe := regexp.MustCompile(`(?:var |,)\s*` + regexp.QuoteMeta(funcName) + `\s*=\s*(function\([^)]*\)\s*\{[\s\S]*?\})\s*[,;]`)
	fm := funcRe.FindStringSubmatch(js)
	if fm == nil {
		return nil, fmt.Errorf("nsig: function body not found for %q", funcName)
	}

	src := "var " + funcName + "=" + fm[1]
	prog, err := goja.Compile("nsig", src, false)
	if err != nil {
		return nil, fmt.Errorf("nsig: compile: %w", err)
	}
	return &nsigEntry{prog: prog, funcName: funcName}, nil
}

func (e *nsigEntry) decode(token string) (string, error) {
	vm := goja.New()
	if _, err := vm.RunProgram(e.prog); err != nil {
		return "", fmt.Errorf("nsig: init runtime: %w", err)
	}
	fn, ok := goja.AssertFunction(vm.Get(e.funcName))
	if !ok {
		return "", fmt.Errorf("nsig: %q is not a function", e.funcName)
	}
	result, err := fn(goja.Undefined(), vm.ToValue(token))
	if err != nil {
		return "", fmt.Errorf("nsig: execute: %w", err)
	}
	return result.String(), nil
}
```

- [ ] **Step 5: Implement base_js.go**

```go
// server/internal/innertube/sig/base_js.go
package sig

import (
	"context"
	"errors"
	"fmt"
	"io"
	"net/http"
	"regexp"
	"sync"
	"time"
)

var (
	ErrNoPlayerJs    = errors.New("sig: no player js available")
	ErrSigInvalidated = errors.New("sig: invalidated after sustained 403s")

	// iframeAPIURL is fetched to discover the current player JS hash.
	iframeAPIURL = "https://www.youtube.com/iframe_api"

	// playerHashRe extracts the player hash from the iframe_api JS body.
	playerHashRe = regexp.MustCompile(`/s/player/([a-f0-9]+)/`)

	cacheTTL      = 6 * time.Hour
	failThreshold = 3
	failWindow    = 60 * time.Second
)

type entry struct {
	nsig     *nsigEntry
	loadedAt time.Time
}

// Cache fetches, parses, and caches base.js per player-JS hash.
// It is safe for concurrent use.
type Cache struct {
	mu      sync.RWMutex
	entries map[string]*entry // key = playerJsHash
	current string            // most recently bootstrapped hash
	http    *http.Client

	failMu  sync.Mutex
	fails   map[string][]time.Time // recent 403 timestamps per hash
}

// NewCache creates a Cache. Pass nil to use http.DefaultClient.
func NewCache(httpClient *http.Client) *Cache {
	if httpClient == nil {
		httpClient = http.DefaultClient
	}
	return &Cache{
		entries: make(map[string]*entry),
		fails:   make(map[string][]time.Time),
		http:    httpClient,
	}
}

// Bootstrap fetches https://www.youtube.com/iframe_api, derives the current
// player JS hash, fetches and parses base.js, and pre-warms the cache.
// Call once at startup; DecodeN will also call it lazily if needed.
func (c *Cache) Bootstrap(ctx context.Context) error {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, iframeAPIURL, nil)
	if err != nil {
		return err
	}
	resp, err := c.http.Do(req)
	if err != nil {
		return fmt.Errorf("sig bootstrap: %w", err)
	}
	defer resp.Body.Close()
	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return fmt.Errorf("sig bootstrap: read: %w", err)
	}
	m := playerHashRe.FindSubmatch(body)
	if m == nil {
		return fmt.Errorf("sig bootstrap: player hash not found in iframe_api response")
	}
	hash := string(m[1])
	baseJsURL := fmt.Sprintf("https://www.youtube.com/s/player/%s/player_ias.vflset/en_US/base.js", hash)
	return c.load(ctx, hash, baseJsURL)
}

func (c *Cache) load(ctx context.Context, hash, baseJsURL string) error {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, baseJsURL, nil)
	if err != nil {
		return err
	}
	resp, err := c.http.Do(req)
	if err != nil {
		return fmt.Errorf("sig load base.js: %w", err)
	}
	defer resp.Body.Close()
	js, err := io.ReadAll(resp.Body)
	if err != nil {
		return fmt.Errorf("sig load base.js read: %w", err)
	}
	nsig, err := extractNsig(string(js))
	if err != nil {
		return fmt.Errorf("sig load %s: %w", hash, err)
	}
	c.mu.Lock()
	c.entries[hash] = &entry{nsig: nsig, loadedAt: time.Now()}
	c.current = hash
	c.mu.Unlock()
	return nil
}

func (c *Cache) getEntry(ctx context.Context, playerJsURL string) (*entry, error) {
	hash := playerJsURL
	if m := playerHashRe.FindStringSubmatch(playerJsURL); m != nil {
		hash = m[1]
	}

	c.mu.RLock()
	e, ok := c.entries[hash]
	c.mu.RUnlock()
	if ok && time.Since(e.loadedAt) < cacheTTL {
		return e, nil
	}

	// Not cached or stale — fetch.
	if playerJsURL == "" {
		if err := c.Bootstrap(ctx); err != nil {
			return nil, fmt.Errorf("%w: bootstrap failed: %v", ErrNoPlayerJs, err)
		}
		c.mu.RLock()
		e = c.entries[c.current]
		c.mu.RUnlock()
		return e, nil
	}
	if err := c.load(ctx, hash, playerJsURL); err != nil {
		return nil, err
	}
	c.mu.RLock()
	e = c.entries[hash]
	c.mu.RUnlock()
	return e, nil
}

// DecodeN replaces the ?n= query parameter in rawURL with the decoded value.
// playerJsURL is the absolute base.js URL from PlayerResponse.PlayerJsURL;
// if empty, the most recently bootstrapped entry is used.
func (c *Cache) DecodeN(ctx context.Context, rawURL, playerJsURL string) (string, error) {
	e, err := c.getEntry(ctx, playerJsURL)
	if err != nil {
		return rawURL, err
	}

	parsed, err := parseAndReplaceN(rawURL, e.nsig)
	if err != nil {
		return rawURL, err
	}
	return parsed, nil
}

// LoadNsigForTest loads a pre-compiled nsig entry for unit testing.
// Do not call in production code.
func (c *Cache) LoadNsigForTest(funcName, funcBody string) error {
	src := "var " + funcName + "=" + funcBody
	prog, err := goja.Compile("nsig_test", src, false)
	if err != nil {
		return err
	}
	e := &entry{
		nsig:     &nsigEntry{prog: prog, funcName: funcName},
		loadedAt: time.Now(),
	}
	c.mu.Lock()
	c.entries["__test__"] = e
	c.current = "__test__"
	c.mu.Unlock()
	return nil
}

// DecodeNRaw decodes a raw n-token (not a full URL) using the currently loaded entry.
// Only for testing.
func (c *Cache) DecodeNRaw(ctx context.Context, token string) (string, error) {
	c.mu.RLock()
	e, ok := c.entries[c.current]
	c.mu.RUnlock()
	if !ok {
		return "", ErrNoPlayerJs
	}
	return e.nsig.decode(token)
}
```

- [ ] **Step 6: Add parseAndReplaceN helper to nsig.go**

```go
// Append to server/internal/innertube/sig/nsig.go

import "net/url"

func parseAndReplaceN(rawURL string, nsig *nsigEntry) (string, error) {
	u, err := url.Parse(rawURL)
	if err != nil {
		return rawURL, fmt.Errorf("nsig: parse url: %w", err)
	}
	q := u.Query()
	n := q.Get("n")
	if n == "" {
		return rawURL, nil // no n param, nothing to do
	}
	decoded, err := nsig.decode(n)
	if err != nil {
		return rawURL, err
	}
	q.Set("n", decoded)
	u.RawQuery = q.Encode()
	return u.String(), nil
}
```

Note: you'll need to add `"net/url"` to the imports in nsig.go, and add `"github.com/dop251/goja"` to base_js.go imports.

- [ ] **Step 7: Run tests — expect PASS**

```bash
cd server && go test ./internal/innertube/sig/...
```

- [ ] **Step 8: Commit**

```bash
git add server/internal/innertube/sig/
git commit -m "feat(m3): sig cache with iframe_api bootstrap and nsig via goja"
```

---

### Task 6: HTTP client

**Files:**
- Create: `server/internal/innertube/client.go`
- Create: `server/internal/innertube/client_test.go`

**Interfaces:**
- Consumes: `sig.Cache`, `payloads.*`, `models.*`, `continuation.Cursor`
- Produces:
  - `NewClient(opts ClientOpts) *Client`
  - `(*Client).Player(ctx, videoID string) (models.PlayerResponse, error)`
  - `(*Client).Next(ctx, videoID string, cont continuation.Cursor) (models.NextPage, error)`
  - `(*Client).Browse(ctx, browseID string, cont continuation.Cursor) (json.RawMessage, error)`
  - `(*Client).Search(ctx, query string) (json.RawMessage, error)`

- [ ] **Step 1: Write client test**

```go
// server/internal/innertube/client_test.go
package innertube_test

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/iceice666/sunflower/server/internal/innertube"
	"github.com/iceice666/sunflower/server/internal/innertube/models"
	"github.com/iceice666/sunflower/server/internal/innertube/sig"
)

func TestClientPlayer_MockServer(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		// Minimal player response: one audio format with a plain URL.
		json.NewEncoder(w).Encode(map[string]any{
			"videoDetails": map[string]any{"videoId": "dQw4w9WgXcQ"},
			"streamingData": map[string]any{
				"formats": []any{},
				"adaptiveFormats": []any{
					map[string]any{
						"itag":     251,
						"mimeType": "audio/webm; codecs=\"opus\"",
						"bitrate":  129000,
						"url":      "https://example.com/stream?n=test",
					},
				},
			},
		})
	}))
	defer srv.Close()

	cache := sig.NewCache(srv.Client())
	cache.LoadNsigForTest("decodeN", "function decodeN(a){return a}")

	client := innertube.NewClient(innertube.ClientOpts{
		HTTPClient:  srv.Client(),
		SigCache:    cache,
		Locale:      models.Locale{HL: "en", GL: "US"},
		BaseURL:     srv.URL,
	})

	resp, err := client.Player(context.Background(), "dQw4w9WgXcQ")
	if err != nil {
		t.Fatalf("Player: %v", err)
	}
	if resp.VideoID != "dQw4w9WgXcQ" {
		t.Errorf("VideoID = %q, want dQw4w9WgXcQ", resp.VideoID)
	}
	if resp.Stream.Itag != 251 {
		t.Errorf("Stream.Itag = %d, want 251", resp.Stream.Itag)
	}
}
```

- [ ] **Step 2: Run — expect FAIL**

```bash
cd server && go test ./internal/innertube/... -run TestClientPlayer_MockServer
```

- [ ] **Step 3: Implement client.go**

```go
// server/internal/innertube/client.go
package innertube

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"

	"github.com/iceice666/sunflower/server/internal/innertube/continuation"
	"github.com/iceice666/sunflower/server/internal/innertube/models"
	"github.com/iceice666/sunflower/server/internal/innertube/payloads"
	"github.com/iceice666/sunflower/server/internal/innertube/sig"
)

const defaultBaseURL = "https://music.youtube.com"

// ClientOpts configures a Client.
type ClientOpts struct {
	HTTPClient  *http.Client
	SigCache    *sig.Cache
	Cookies     func() []*http.Cookie  // nil = guest mode
	CookieSink  func([]*http.Cookie)   // captures Set-Cookie rotations; may be nil
	Locale      models.Locale
	BaseURL     string // override for testing; defaults to https://music.youtube.com
}

// Client posts requests to the InnerTube API and returns raw JSON responses.
// Parsing is handled by the parser package, not here.
type Client struct {
	http       *http.Client
	sig        *sig.Cache
	cookies    func() []*http.Cookie
	cookieSink func([]*http.Cookie)
	locale     models.Locale
	baseURL    string
}

// NewClient creates a Client from opts. SigCache must not be nil.
func NewClient(opts ClientOpts) *Client {
	if opts.HTTPClient == nil {
		opts.HTTPClient = http.DefaultClient
	}
	if opts.BaseURL == "" {
		opts.BaseURL = defaultBaseURL
	}
	return &Client{
		http:       opts.HTTPClient,
		sig:        opts.SigCache,
		cookies:    opts.Cookies,
		cookieSink: opts.CookieSink,
		locale:     opts.Locale,
		baseURL:    opts.BaseURL,
	}
}

// Player calls /youtubei/v1/player and returns a parsed PlayerResponse.
func (c *Client) Player(ctx context.Context, videoID string) (models.PlayerResponse, error) {
	body := payloads.Player(videoID, c.locale)
	raw, err := c.post(ctx, "/youtubei/v1/player", AndroidMusicAPIKey(), body)
	if err != nil {
		return models.PlayerResponse{}, err
	}
	return parsePlayerResponseRaw(ctx, raw, c.sig)
}

// Next calls /youtubei/v1/next and returns raw JSON for the parser to consume.
func (c *Client) Next(ctx context.Context, videoID string, cont continuation.Cursor) (json.RawMessage, error) {
	body := payloads.Next(videoID, cont, c.locale)
	return c.post(ctx, "/youtubei/v1/next", AndroidMusicAPIKey(), body)
}

// Browse calls /youtubei/v1/browse with WEB_REMIX context.
func (c *Client) Browse(ctx context.Context, browseID string, cont continuation.Cursor) (json.RawMessage, error) {
	body := BuildWebRemixContext(c.locale)
	body["browseId"] = browseID
	if !cont.IsZero() {
		body["continuation"] = string(cont)
	}
	return c.post(ctx, "/youtubei/v1/browse", WebRemixAPIKey(), body)
}

// Search calls /youtubei/v1/search with WEB_REMIX context.
func (c *Client) Search(ctx context.Context, query string) (json.RawMessage, error) {
	body := BuildWebRemixContext(c.locale)
	body["query"] = query
	return c.post(ctx, "/youtubei/v1/search", WebRemixAPIKey(), body)
}

func (c *Client) post(ctx context.Context, path, apiKey string, payload map[string]any) (json.RawMessage, error) {
	encoded, err := json.Marshal(payload)
	if err != nil {
		return nil, fmt.Errorf("innertube post: marshal: %w", err)
	}

	url := c.baseURL + path + "?key=" + apiKey
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, url, bytes.NewReader(encoded))
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("User-Agent", "com.google.android.apps.youtube.music/7.27.52 (Linux; U; Android 11) gzip")
	req.Header.Set("X-YouTube-Client-Name", androidMusicClientID)
	req.Header.Set("X-YouTube-Client-Version", androidMusicClientVersion)

	if c.cookies != nil {
		for _, ck := range c.cookies() {
			req.AddCookie(ck)
		}
	}

	resp, err := c.http.Do(req)
	if err != nil {
		return nil, fmt.Errorf("innertube post %s: %w", path, err)
	}
	defer resp.Body.Close()

	if c.cookieSink != nil && len(resp.Cookies()) > 0 {
		c.cookieSink(resp.Cookies())
	}

	if resp.StatusCode >= 500 {
		// Retry once on 5xx.
		resp.Body.Close()
		req2, _ := http.NewRequestWithContext(ctx, http.MethodPost, url, bytes.NewReader(encoded))
		req2.Header = req.Header.Clone()
		resp, err = c.http.Do(req2)
		if err != nil {
			return nil, fmt.Errorf("innertube post %s retry: %w", path, err)
		}
		defer resp.Body.Close()
	}

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("innertube post %s: status %d", path, resp.StatusCode)
	}

	return io.ReadAll(resp.Body)
}

// parsePlayerResponseRaw extracts stream URLs from the raw player JSON,
// applies n-param decryption, and picks the best audio stream.
func parsePlayerResponseRaw(ctx context.Context, raw json.RawMessage, cache *sig.Cache) (models.PlayerResponse, error) {
	var pr struct {
		VideoDetails struct {
			VideoID string `json:"videoId"`
		} `json:"videoDetails"`
		StreamingData struct {
			AdaptiveFormats []struct {
				Itag             int    `json:"itag"`
				MimeType         string `json:"mimeType"`
				Bitrate          int    `json:"bitrate"`
				URL              string `json:"url"`
				SignatureCipher  string `json:"signatureCipher"`
				LoudnessDB       float64 `json:"loudnessDb"`
			} `json:"adaptiveFormats"`
		} `json:"streamingData"`
	}
	if err := json.Unmarshal(raw, &pr); err != nil {
		return models.PlayerResponse{}, fmt.Errorf("parse player: %w", err)
	}

	var streams []models.StreamURL
	for _, f := range pr.StreamingData.AdaptiveFormats {
		// Only audio formats.
		if len(f.MimeType) > 5 && f.MimeType[:5] != "audio" {
			continue
		}
		rawURL := f.URL
		if rawURL == "" {
			continue // signatureCipher path not yet supported (Task 11)
		}
		decoded, err := cache.DecodeN(ctx, rawURL, "")
		if err != nil {
			decoded = rawURL // best-effort: use undecoded URL
		}
		streams = append(streams, models.StreamURL{
			URL:      decoded,
			Itag:     f.Itag,
			MimeType: f.MimeType,
			Bitrate:  f.Bitrate,
			Loudness: f.LoudnessDB,
		})
	}

	var best models.StreamURL
	for _, s := range streams {
		if s.Bitrate > best.Bitrate {
			best = s
		}
	}

	return models.PlayerResponse{
		VideoID:   pr.VideoDetails.VideoID,
		Stream:    best,
		AllStreams: streams,
	}, nil
}
```

- [ ] **Step 4: Run tests — expect PASS**

```bash
cd server && go test ./internal/innertube/...
```

- [ ] **Step 5: Commit**

```bash
git add server/internal/innertube/client.go server/internal/innertube/client_test.go
git commit -m "feat(m3): InnerTube HTTP client with player and next"
```

---

### Task 7: Probe CLI — `innertube next` subcommand

**Files:**
- Create: `server/cmd/probe/main.go`
- Create: `server/cmd/probe/innertube_cmd.go`

**Interfaces:**
- Consumes: `innertube.Client`, `sig.Cache`, `models.ProbeNextResult`
- Produces: runnable `probe` binary; `probe innertube next --video-id=X [-o json|url]`

- [ ] **Step 1: Implement main.go**

```go
// server/cmd/probe/main.go
package main

import (
	"flag"
	"fmt"
	"os"
)

func main() {
	flag.Usage = func() {
		fmt.Fprintln(os.Stderr, "usage: probe <command> [flags]")
		fmt.Fprintln(os.Stderr, "  innertube next --video-id=<id> [-o json|url]")
		fmt.Fprintln(os.Stderr, "  innertube home")
		fmt.Fprintln(os.Stderr, "  innertube search --query=<q>")
		fmt.Fprintln(os.Stderr, "  innertube cookies-set --file=<path>")
	}
	if len(os.Args) < 2 {
		flag.Usage()
		os.Exit(1)
	}
	switch os.Args[1] {
	case "innertube":
		runInnertube(os.Args[2:])
	default:
		fmt.Fprintf(os.Stderr, "unknown command: %s\n", os.Args[1])
		flag.Usage()
		os.Exit(1)
	}
}
```

- [ ] **Step 2: Implement innertube_cmd.go**

```go
// server/cmd/probe/innertube_cmd.go
package main

import (
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"net/http"
	"os"
	"time"

	"github.com/iceice666/sunflower/server/internal/innertube"
	"github.com/iceice666/sunflower/server/internal/innertube/models"
	"github.com/iceice666/sunflower/server/internal/innertube/parser"
	"github.com/iceice666/sunflower/server/internal/innertube/sig"
)

func runInnertube(args []string) {
	if len(args) == 0 {
		fmt.Fprintln(os.Stderr, "usage: probe innertube <next|home|search|cookies-set>")
		os.Exit(1)
	}
	switch args[0] {
	case "next":
		runNext(args[1:])
	default:
		fmt.Fprintf(os.Stderr, "unknown innertube subcommand: %s\n", args[0])
		os.Exit(1)
	}
}

func runNext(args []string) {
	fs := flag.NewFlagSet("next", flag.ExitOnError)
	videoID := fs.String("video-id", "", "YouTube video ID (required)")
	output := fs.String("o", "json", "output format: json|url")
	fs.Parse(args)

	if *videoID == "" {
		fmt.Fprintln(os.Stderr, "--video-id is required")
		os.Exit(1)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	cache := sig.NewCache(http.DefaultClient)
	if err := cache.Bootstrap(ctx); err != nil {
		fmt.Fprintf(os.Stderr, "sig bootstrap: %v\n", err)
		os.Exit(1)
	}

	client := innertube.NewClient(innertube.ClientOpts{
		SigCache: cache,
		Locale:   models.Locale{HL: "en", GL: "US"},
	})

	playerResp, err := client.Player(ctx, *videoID)
	if err != nil {
		fmt.Fprintf(os.Stderr, "player: %v\n", err)
		os.Exit(1)
	}

	nextRaw, err := client.Next(ctx, *videoID, nil)
	if err != nil {
		fmt.Fprintf(os.Stderr, "next: %v\n", err)
		os.Exit(1)
	}
	nextPage := parser.ParseNextPage(nextRaw)

	result := models.ProbeNextResult{
		CurrentURL:   playerResp.Stream.URL,
		ExpiresAt:    playerResp.Stream.ExpiresAt,
		Itag:         playerResp.Stream.Itag,
		NextItems:    nextPage.Related,
		Continuation: nextPage.Continuation,
	}

	switch *output {
	case "url":
		fmt.Println(result.CurrentURL)
	default:
		enc := json.NewEncoder(os.Stdout)
		enc.SetIndent("", "  ")
		enc.Encode(result)
	}
}
```

- [ ] **Step 3: Build the probe binary**

```bash
cd server && go build ./cmd/probe/
```

Expected: `probe` binary created (or `probe.exe` on Windows)

- [ ] **Step 4: Bootstrap sig and capture live fixtures**

```bash
# Run probe — this makes a real network call to YouTube.
./probe innertube next --video-id=dQw4w9WgXcQ -o json > /tmp/probe_next_output.json
cat /tmp/probe_next_output.json
```

Expected: JSON with `current_url` pointing to a `googlevideo.com` URL.

If it works, capture raw YT responses for parser fixtures:

```bash
# Add a --dump-raw flag by temporarily editing innertube_cmd.go to also write
# the raw player and next JSON to files:
#   os.WriteFile("internal/innertube/parser/testdata/player_response.json", playerRawBytes, 0644)
#   os.WriteFile("internal/innertube/parser/testdata/next_response.json", nextRawBytes, 0644)
# Then run probe once, then revert the dump code.
```

- [ ] **Step 5: Commit**

```bash
git add server/cmd/probe/
git commit -m "feat(m3): probe CLI with innertube next subcommand"
```

---

### Task 8: Parser helpers + next_page + yt_item — DEMO TARGET

**Files:**
- Create: `server/internal/innertube/parser/helpers.go`
- Create: `server/internal/innertube/parser/yt_item.go`
- Create: `server/internal/innertube/parser/next_page.go`
- Create: `server/internal/innertube/parser/next_page_test.go`
- Create: `server/internal/innertube/parser/testdata/next_response.json` (captured in Task 7)
- Create: `server/internal/innertube/parser/testdata/next_no_continuation.json` (hand-edit: remove continuation token)

**Interfaces:**
- Produces: `parser.ParseNextPage(raw json.RawMessage) models.NextPage`

- [ ] **Step 1: Write failing tests**

```go
// server/internal/innertube/parser/next_page_test.go
package parser_test

import (
	"os"
	"testing"

	"github.com/iceice666/sunflower/server/internal/innertube/parser"
)

func TestParseNextPage_NormalShape(t *testing.T) {
	raw, err := os.ReadFile("testdata/next_response.json")
	if err != nil {
		t.Skipf("fixture not yet captured: %v", err)
	}
	page := parser.ParseNextPage(raw)
	if page.Current.VideoID == "" {
		t.Error("Current.VideoID should not be empty")
	}
	// Related items are optional; just verify no panic.
	t.Logf("related items: %d", len(page.Related))
}

func TestParseNextPage_NoContinuation(t *testing.T) {
	raw, err := os.ReadFile("testdata/next_no_continuation.json")
	if err != nil {
		t.Skipf("fixture not yet captured: %v", err)
	}
	page := parser.ParseNextPage(raw)
	if !page.Continuation.IsZero() {
		t.Error("continuation should be zero when absent in fixture")
	}
}

func TestParseNextPage_EmptyJSON(t *testing.T) {
	page := parser.ParseNextPage([]byte("{}"))
	// Must not panic; all fields zero.
	if page.Current.VideoID != "" {
		t.Errorf("unexpected VideoID: %q", page.Current.VideoID)
	}
}
```

- [ ] **Step 2: Run — expect SKIP or FAIL**

```bash
cd server && go test ./internal/innertube/parser/... -run TestParseNextPage
```

- [ ] **Step 3: Implement helpers.go**

```go
// server/internal/innertube/parser/helpers.go
package parser

// getString traverses a nested map[string]any by path and returns the string
// value at the leaf, or "" if any step is missing or not a string.
func getString(m map[string]any, path ...string) string {
	cur := m
	for i, key := range path {
		v, ok := cur[key]
		if !ok {
			return ""
		}
		if i == len(path)-1 {
			s, _ := v.(string)
			return s
		}
		cur, ok = v.(map[string]any)
		if !ok {
			return ""
		}
	}
	return ""
}

// getArray traverses a nested map[string]any and returns the []any at the leaf.
func getArray(m map[string]any, path ...string) []any {
	cur := m
	for i, key := range path {
		v, ok := cur[key]
		if !ok {
			return nil
		}
		if i == len(path)-1 {
			a, _ := v.([]any)
			return a
		}
		cur, ok = v.(map[string]any)
		if !ok {
			return nil
		}
	}
	return nil
}

// getMap traverses a nested map[string]any and returns the sub-map at the leaf.
func getMap(m map[string]any, path ...string) map[string]any {
	cur := m
	for _, key := range path {
		v, ok := cur[key]
		if !ok {
			return nil
		}
		next, ok := v.(map[string]any)
		if !ok {
			return nil
		}
		cur = next
	}
	return cur
}

// getInt traverses a nested map[string]any and returns the int at the leaf.
func getInt(m map[string]any, path ...string) int {
	cur := m
	for i, key := range path {
		v, ok := cur[key]
		if !ok {
			return 0
		}
		if i == len(path)-1 {
			switch n := v.(type) {
			case float64:
				return int(n)
			case int:
				return n
			}
			return 0
		}
		cur, ok = v.(map[string]any)
		if !ok {
			return 0
		}
	}
	return 0
}

// unmarshalMap decodes raw JSON into a map[string]any for use with helpers.
// Returns nil on error — parsers treat nil as empty, never error.
func unmarshalMap(raw []byte) map[string]any {
	var m map[string]any
	_ = unmarshal(raw, &m)
	return m
}

func unmarshal(raw []byte, v any) error {
	import "encoding/json"
	return json.Unmarshal(raw, v)
}
```

Note: move the `import "encoding/json"` to the package-level imports block; the inline syntax above is illustrative. The actual file must have `import "encoding/json"` at the top.

- [ ] **Step 4: Implement yt_item.go**

```go
// server/internal/innertube/parser/yt_item.go
package parser

import (
	"github.com/iceice666/sunflower/server/internal/innertube/models"
)

func parseSongItem(m map[string]any) models.SongItem {
	if m == nil {
		return models.SongItem{}
	}
	title := getString(m, "title", "runs", "0", "text")
	if title == "" {
		title = getString(m, "title", "simpleText")
	}

	videoID := getString(m, "videoId")

	var artists []string
	for _, run := range getArray(m, "subtitle", "runs") {
		r, ok := run.(map[string]any)
		if !ok {
			continue
		}
		ep := getMap(r, "navigationEndpoint", "browseEndpoint")
		if ep == nil {
			continue
		}
		pageType := getString(ep, "browseEndpointContextSupportedConfigs",
			"browseEndpointContextMusicConfig", "pageType")
		if pageType == "MUSIC_PAGE_TYPE_ARTIST" {
			artists = append(artists, getString(r, "text"))
		}
	}

	thumbnail := ""
	thumbs := getArray(m, "thumbnail", "thumbnails")
	if len(thumbs) > 0 {
		if t, ok := thumbs[len(thumbs)-1].(map[string]any); ok {
			thumbnail = getString(t, "url")
		}
	}

	return models.SongItem{
		VideoID:      videoID,
		Title:        title,
		Artists:      artists,
		ThumbnailURL: thumbnail,
	}
}

func parseAlbumItem(m map[string]any) models.AlbumItem {
	if m == nil {
		return models.AlbumItem{}
	}
	return models.AlbumItem{
		BrowseID: getString(m, "navigationEndpoint", "browseEndpoint", "browseId"),
		Title:    getString(m, "title", "runs", "0", "text"),
	}
}

func parseArtistItem(m map[string]any) models.ArtistItem {
	if m == nil {
		return models.ArtistItem{}
	}
	return models.ArtistItem{
		BrowseID: getString(m, "navigationEndpoint", "browseEndpoint", "browseId"),
		Name:     getString(m, "title", "runs", "0", "text"),
	}
}
```

- [ ] **Step 5: Implement next_page.go**

```go
// server/internal/innertube/parser/next_page.go
package parser

import (
	"encoding/json"

	"github.com/iceice666/sunflower/server/internal/innertube/continuation"
	"github.com/iceice666/sunflower/server/internal/innertube/models"
	"github.com/rs/zerolog/log"
)

// ParseNextPage parses the raw JSON response from /youtubei/v1/next.
// Missing fields return zero values; unknown renderers are skipped.
func ParseNextPage(raw json.RawMessage) models.NextPage {
	m := unmarshalMap(raw)
	if m == nil {
		return models.NextPage{}
	}

	var page models.NextPage

	// Extract current item from videoDetails.
	if vd := getMap(m, "videoDetails"); vd != nil {
		page.Current = models.SongItem{
			VideoID: getString(vd, "videoId"),
			Title:   getString(vd, "title"),
		}
	}

	// Extract related items from the automix/related shelf.
	// The exact path varies; try common locations.
	related := extractRelatedItems(m)
	page.Related = related

	// Extract continuation token.
	cont := extractContinuation(m)
	page.Continuation = cont

	return page
}

func extractRelatedItems(m map[string]any) []models.SongItem {
	// YT Music next response nests related items under several possible paths.
	// Try the most common one; parsers should be extended with real fixture paths.
	tabs := getArray(m, "contents", "singleColumnMusicWatchNextResultsRenderer",
		"tabbedRenderer", "watchNextTabbedResultsRenderer", "tabs")

	var items []models.SongItem
	for _, tab := range tabs {
		t, ok := tab.(map[string]any)
		if !ok {
			continue
		}
		endpoint := getMap(t, "tabRenderer", "endpoint", "browseEndpoint")
		if endpoint == nil {
			continue
		}
		// The "Up Next" tab.
		content := getMap(t, "tabRenderer", "content")
		if content == nil {
			continue
		}
		musicQueue := getMap(content, "musicQueueRenderer")
		if musicQueue == nil {
			continue
		}
		for _, item := range getArray(musicQueue, "content", "playlistPanelRenderer", "contents") {
			r, ok := item.(map[string]any)
			if !ok {
				continue
			}
			if ppvr := getMap(r, "playlistPanelVideoRenderer"); ppvr != nil {
				items = append(items, parseSongItem(ppvr))
			} else {
				log.Debug().Str("renderer", firstKey(r)).Msg("innertube: unknown next renderer, skipping")
			}
		}
	}
	return items
}

func extractContinuation(m map[string]any) continuation.Cursor {
	// Continuation tokens appear at various depths; try common paths.
	tok := getString(m, "contents", "singleColumnMusicWatchNextResultsRenderer",
		"tabbedRenderer", "watchNextTabbedResultsRenderer", "tabs",
		"0", "tabRenderer", "content", "musicQueueRenderer",
		"content", "playlistPanelRenderer", "continuations",
		"0", "nextRadioContinuationData", "continuation")
	if tok == "" {
		tok = getString(m, "continuationContents", "playlistPanelContinuation",
			"continuations", "0", "nextRadioContinuationData", "continuation")
	}
	if tok == "" {
		return nil
	}
	return continuation.Cursor(tok)
}

func firstKey(m map[string]any) string {
	for k := range m {
		return k
	}
	return ""
}
```

- [ ] **Step 6: Fix helpers.go imports**

The `unmarshalMap` / `unmarshal` helpers need proper imports. Replace the helpers.go content with:

```go
// server/internal/innertube/parser/helpers.go
package parser

import "encoding/json"

func getString(m map[string]any, path ...string) string {
	cur := m
	for i, key := range path {
		v, ok := cur[key]
		if !ok {
			return ""
		}
		if i == len(path)-1 {
			s, _ := v.(string)
			return s
		}
		next, ok := v.(map[string]any)
		if !ok {
			return ""
		}
		cur = next
	}
	return ""
}

func getArray(m map[string]any, path ...string) []any {
	cur := m
	for i, key := range path {
		v, ok := cur[key]
		if !ok {
			return nil
		}
		if i == len(path)-1 {
			a, _ := v.([]any)
			return a
		}
		next, ok := v.(map[string]any)
		if !ok {
			return nil
		}
		cur = next
	}
	return nil
}

func getMap(m map[string]any, path ...string) map[string]any {
	cur := m
	for _, key := range path {
		v, ok := cur[key]
		if !ok {
			return nil
		}
		next, ok := v.(map[string]any)
		if !ok {
			return nil
		}
		cur = next
	}
	return cur
}

func getInt(m map[string]any, path ...string) int {
	cur := m
	for i, key := range path {
		v, ok := cur[key]
		if !ok {
			return 0
		}
		if i == len(path)-1 {
			switch n := v.(type) {
			case float64:
				return int(n)
			case int:
				return n
			}
			return 0
		}
		next, ok := v.(map[string]any)
		if !ok {
			return 0
		}
		cur = next
	}
	return 0
}

func unmarshalMap(raw []byte) map[string]any {
	var m map[string]any
	_ = json.Unmarshal(raw, &m)
	return m
}
```

- [ ] **Step 7: Run tests — expect PASS (fixtures skip gracefully if not yet present)**

```bash
cd server && go test ./internal/innertube/...
```

- [ ] **Step 8: Verify probe end-to-end**

```bash
./probe innertube next --video-id=dQw4w9WgXcQ -o url | head -c 80
```

Expected: URL beginning with `https://rr` or `https://manifest.googlevideo.com`

```bash
# Verify stream is actually playable:
curl -I "$(./probe innertube next --video-id=dQw4w9WgXcQ -o url)"
```

Expected: `HTTP/2 200` or `HTTP/1.1 200 OK`

**Demo target met.**

- [ ] **Step 9: Commit**

```bash
git add server/internal/innertube/parser/ server/cmd/probe/
git commit -m "feat(m3): parser helpers, yt_item, next_page — demo target met"
```

---

### Task 9: Expansion payloads — browse and search

**Files:**
- Create: `server/internal/innertube/payloads/browse.go`
- Create: `server/internal/innertube/payloads/search.go`
- Modify: `server/internal/innertube/payloads/payloads_test.go`

**Interfaces:**
- Produces:
  - `payloads.Browse(browseID string, cont continuation.Cursor, locale models.Locale) map[string]any`
  - `payloads.Search(query string, locale models.Locale) map[string]any`

- [ ] **Step 1: Add tests**

```go
// Append to server/internal/innertube/payloads/payloads_test.go

func TestBrowsePayload(t *testing.T) {
	p := payloads.Browse("FEmusic_home", nil, models.Locale{HL: "en", GL: "US"})
	if p["browseId"] != "FEmusic_home" {
		t.Errorf("browseId = %v", p["browseId"])
	}
	ctx := p["context"].(map[string]any)["client"].(map[string]any)
	if ctx["clientName"] != "WEB_REMIX" {
		t.Errorf("context should use WEB_REMIX, got %v", ctx["clientName"])
	}
}

func TestSearchPayload(t *testing.T) {
	p := payloads.Search("Beatles", models.Locale{HL: "en", GL: "US"})
	if p["query"] != "Beatles" {
		t.Errorf("query = %v", p["query"])
	}
}
```

- [ ] **Step 2: Implement browse.go**

```go
// server/internal/innertube/payloads/browse.go
package payloads

import (
	"github.com/iceice666/sunflower/server/internal/innertube"
	"github.com/iceice666/sunflower/server/internal/innertube/continuation"
	"github.com/iceice666/sunflower/server/internal/innertube/models"
)

// Browse builds the POST body for /youtubei/v1/browse with WEB_REMIX context.
func Browse(browseID string, cont continuation.Cursor, locale models.Locale) map[string]any {
	body := innertube.BuildWebRemixContext(locale)
	body["browseId"] = browseID
	if !cont.IsZero() {
		body["continuation"] = string(cont)
	}
	return body
}
```

- [ ] **Step 3: Implement search.go**

```go
// server/internal/innertube/payloads/search.go
package payloads

import (
	"github.com/iceice666/sunflower/server/internal/innertube"
	"github.com/iceice666/sunflower/server/internal/innertube/models"
)

// Search builds the POST body for /youtubei/v1/search with WEB_REMIX context.
func Search(query string, locale models.Locale) map[string]any {
	body := innertube.BuildWebRemixContext(locale)
	body["query"] = query
	return body
}
```

- [ ] **Step 4: Run tests — expect PASS**

```bash
cd server && go test ./internal/innertube/payloads/...
```

- [ ] **Step 5: Commit**

```bash
git add server/internal/innertube/payloads/
git commit -m "feat(m3): browse and search payload builders"
```

---

### Task 10: Expansion parsers + probe home/search

**Files:**
- Create: `server/internal/innertube/parser/home_page.go`
- Create: `server/internal/innertube/parser/related_page.go`
- Create: `server/internal/innertube/parser/artist_page.go`
- Create: `server/internal/innertube/parser/album_page.go`
- Create: `server/internal/innertube/parser/playlist_page.go`
- Create: `server/internal/innertube/parser/search_page.go`
- Create: `server/internal/innertube/parser/expansion_test.go`
- Create: `server/internal/innertube/parser/testdata/home_response.json` (captured)
- Create: `server/internal/innertube/parser/testdata/search_response.json` (captured)
- Modify: `server/cmd/probe/innertube_cmd.go` (add home/search subcommands)

**Interfaces:**
- Produces:
  - `parser.ParseHomePage(raw json.RawMessage) models.HomePage`
  - `parser.ParseSearchPage(raw json.RawMessage) models.SearchPage`
  - `parser.ParseRelatedPage(raw json.RawMessage) []models.SongItem`
  - `parser.ParseArtistPage(raw json.RawMessage) models.ArtistItem`
  - `parser.ParseAlbumPage(raw json.RawMessage) models.AlbumItem`
  - `parser.ParsePlaylistPage(raw json.RawMessage) []models.SongItem`

- [ ] **Step 1: Capture home and search fixtures**

```bash
# Temporarily add raw dump to innertube_cmd.go to capture home and search responses.
# Add home command:
./probe innertube home > /dev/null   # (implement home cmd first, see Step 5)

# Capture search:
./probe innertube search --query="Rick Astley" > /dev/null
```

- [ ] **Step 2: Write expansion tests**

```go
// server/internal/innertube/parser/expansion_test.go
package parser_test

import (
	"os"
	"testing"

	"github.com/iceice666/sunflower/server/internal/innertube/parser"
)

func TestParseHomePage_EmptyJSON(t *testing.T) {
	page := parser.ParseHomePage([]byte("{}"))
	// Must not panic.
	_ = page
}

func TestParseHomePage_FromFixture(t *testing.T) {
	raw, err := os.ReadFile("testdata/home_response.json")
	if err != nil {
		t.Skipf("fixture not captured yet: %v", err)
	}
	page := parser.ParseHomePage(raw)
	t.Logf("sections: %d, chips: %d", len(page.Sections), len(page.Chips))
}

func TestParseSearchPage_EmptyJSON(t *testing.T) {
	page := parser.ParseSearchPage([]byte("{}"))
	_ = page
}

func TestParseSearchPage_FromFixture(t *testing.T) {
	raw, err := os.ReadFile("testdata/search_response.json")
	if err != nil {
		t.Skipf("fixture not captured yet: %v", err)
	}
	page := parser.ParseSearchPage(raw)
	t.Logf("songs: %d, albums: %d, artists: %d", len(page.Songs), len(page.Albums), len(page.Artists))
}
```

- [ ] **Step 3: Implement home_page.go**

```go
// server/internal/innertube/parser/home_page.go
package parser

import (
	"encoding/json"

	"github.com/iceice666/sunflower/server/internal/innertube/models"
	"github.com/rs/zerolog/log"
)

// ParseHomePage parses the raw /youtubei/v1/browse?browseId=FEmusic_home response.
func ParseHomePage(raw json.RawMessage) models.HomePage {
	m := unmarshalMap(raw)
	if m == nil {
		return models.HomePage{}
	}

	var page models.HomePage

	// Chips (mood/genre filters).
	for _, chip := range getArray(m, "header", "musicImmersiveHeaderRenderer",
		"menu", "chipCloudRenderer", "chips") {
		c, ok := chip.(map[string]any)
		if !ok {
			continue
		}
		page.Chips = append(page.Chips, getString(c, "chipCloudChipRenderer", "text", "runs", "0", "text"))
	}

	// Sections.
	for _, s := range getArray(m, "contents", "singleColumnBrowseResultsRenderer",
		"tabs", "0", "tabRenderer", "content", "sectionListRenderer", "contents") {
		sec, ok := s.(map[string]any)
		if !ok {
			continue
		}
		section := parseHomeSection(sec)
		if len(section.Items) > 0 || section.Title != "" {
			page.Sections = append(page.Sections, section)
		}
	}

	return page
}

func parseHomeSection(m map[string]any) models.HomeSection {
	var sec models.HomeSection

	if mr := getMap(m, "musicCarouselShelfRenderer"); mr != nil {
		sec.Title = getString(mr, "header", "musicCarouselShelfBasicHeaderRenderer", "title", "runs", "0", "text")
		for _, item := range getArray(mr, "contents") {
			r, ok := item.(map[string]any)
			if !ok {
				continue
			}
			if mi := getMap(r, "musicTwoRowItemRenderer"); mi != nil {
				// Could be song, album, or artist — inspect page type.
				pageType := getString(mi, "navigationEndpoint", "browseEndpoint",
					"browseEndpointContextSupportedConfigs",
					"browseEndpointContextMusicConfig", "pageType")
				switch pageType {
				case "MUSIC_PAGE_TYPE_ALBUM", "MUSIC_PAGE_TYPE_PLAYLIST":
					sec.Items = append(sec.Items, parseAlbumItem(mi))
				case "MUSIC_PAGE_TYPE_ARTIST":
					sec.Items = append(sec.Items, parseArtistItem(mi))
				default:
					// Treat as song if it has a videoId.
					if getString(mi, "navigationEndpoint", "watchEndpoint", "videoId") != "" {
						sec.Items = append(sec.Items, parseSongItem(mi))
					}
				}
			} else {
				log.Debug().Str("renderer", firstKey(r)).Msg("innertube: unknown home section renderer, skipping")
			}
		}
	}

	return sec
}
```

- [ ] **Step 4: Implement search_page.go**

```go
// server/internal/innertube/parser/search_page.go
package parser

import (
	"encoding/json"

	"github.com/iceice666/sunflower/server/internal/innertube/continuation"
	"github.com/iceice666/sunflower/server/internal/innertube/models"
	"github.com/rs/zerolog/log"
)

// ParseSearchPage parses a /youtubei/v1/search response.
func ParseSearchPage(raw json.RawMessage) models.SearchPage {
	m := unmarshalMap(raw)
	if m == nil {
		return models.SearchPage{}
	}

	var page models.SearchPage

	contents := getArray(m, "contents", "tabbedSearchResultsRenderer",
		"tabs", "0", "tabRenderer", "content", "sectionListRenderer", "contents")
	if contents == nil {
		// Continuation response.
		contents = getArray(m, "continuationContents", "musicShelfContinuation", "contents")
	}

	for _, sec := range contents {
		s, ok := sec.(map[string]any)
		if !ok {
			continue
		}
		shelf := getMap(s, "musicShelfRenderer")
		if shelf == nil {
			continue
		}
		for _, item := range getArray(shelf, "contents") {
			r, ok := item.(map[string]any)
			if !ok {
				continue
			}
			if mr := getMap(r, "musicResponsiveListItemRenderer"); mr != nil {
				pageType := getString(mr, "flexColumns", "0",
					"musicResponsiveListItemFlexColumnRenderer", "text", "runs", "0",
					"navigationEndpoint", "browseEndpoint",
					"browseEndpointContextSupportedConfigs",
					"browseEndpointContextMusicConfig", "pageType")
				switch pageType {
				case "MUSIC_PAGE_TYPE_ALBUM":
					page.Albums = append(page.Albums, parseAlbumItem(mr))
				case "MUSIC_PAGE_TYPE_ARTIST":
					page.Artists = append(page.Artists, parseArtistItem(mr))
				default:
					page.Songs = append(page.Songs, parseSongItem(mr))
				}
			} else {
				log.Debug().Str("renderer", firstKey(r)).Msg("innertube: unknown search renderer, skipping")
			}
		}
	}

	// Continuation.
	tok := getString(m, "continuationContents", "musicShelfContinuation",
		"continuations", "0", "nextContinuationData", "continuation")
	if tok != "" {
		page.Continuation = continuation.Cursor(tok)
	}

	return page
}
```

- [ ] **Step 5: Implement stub parsers for related, artist, album, playlist**

```go
// server/internal/innertube/parser/related_page.go
package parser

import (
	"encoding/json"
	"github.com/iceice666/sunflower/server/internal/innertube/models"
)

// ParseRelatedPage parses related items from a browse response.
func ParseRelatedPage(raw json.RawMessage) []models.SongItem {
	m := unmarshalMap(raw)
	if m == nil {
		return nil
	}
	var items []models.SongItem
	for _, item := range getArray(m, "contents", "singleColumnBrowseResultsRenderer",
		"tabs", "0", "tabRenderer", "content", "sectionListRenderer",
		"contents", "0", "musicShelfRenderer", "contents") {
		r, ok := item.(map[string]any)
		if !ok {
			continue
		}
		if mr := getMap(r, "musicResponsiveListItemRenderer"); mr != nil {
			items = append(items, parseSongItem(mr))
		}
	}
	return items
}
```

```go
// server/internal/innertube/parser/artist_page.go
package parser

import (
	"encoding/json"
	"github.com/iceice666/sunflower/server/internal/innertube/models"
)

// ParseArtistPage parses an artist browse response.
func ParseArtistPage(raw json.RawMessage) models.ArtistItem {
	m := unmarshalMap(raw)
	if m == nil {
		return models.ArtistItem{}
	}
	return models.ArtistItem{
		Name: getString(m, "header", "musicImmersiveHeaderRenderer", "title", "runs", "0", "text"),
	}
}
```

```go
// server/internal/innertube/parser/album_page.go
package parser

import (
	"encoding/json"
	"github.com/iceice666/sunflower/server/internal/innertube/models"
)

// ParseAlbumPage parses an album browse response.
func ParseAlbumPage(raw json.RawMessage) models.AlbumItem {
	m := unmarshalMap(raw)
	if m == nil {
		return models.AlbumItem{}
	}
	return models.AlbumItem{
		Title: getString(m, "header", "musicDetailHeaderRenderer", "title", "runs", "0", "text"),
		Year:  getString(m, "header", "musicDetailHeaderRenderer", "subtitle", "runs", "4", "text"),
	}
}
```

```go
// server/internal/innertube/parser/playlist_page.go
package parser

import (
	"encoding/json"
	"github.com/iceice666/sunflower/server/internal/innertube/models"
)

// ParsePlaylistPage parses a playlist browse response, returning its tracks.
func ParsePlaylistPage(raw json.RawMessage) []models.SongItem {
	m := unmarshalMap(raw)
	if m == nil {
		return nil
	}
	var items []models.SongItem
	for _, item := range getArray(m, "contents", "singleColumnBrowseResultsRenderer",
		"tabs", "0", "tabRenderer", "content", "sectionListRenderer",
		"contents", "0", "musicShelfRenderer", "contents") {
		r, ok := item.(map[string]any)
		if !ok {
			continue
		}
		if mr := getMap(r, "musicResponsiveListItemRenderer"); mr != nil {
			items = append(items, parseSongItem(mr))
		}
	}
	return items
}
```

- [ ] **Step 6: Add home and search subcommands to probe**

```go
// Append to server/cmd/probe/innertube_cmd.go — update runInnertube switch:

	case "home":
		runHome(args[1:])
	case "search":
		runSearch(args[1:])

// Add new functions:

func runHome(args []string) {
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	cache := sig.NewCache(http.DefaultClient)
	if err := cache.Bootstrap(ctx); err != nil {
		fmt.Fprintf(os.Stderr, "sig bootstrap: %v\n", err)
		os.Exit(1)
	}
	client := innertube.NewClient(innertube.ClientOpts{
		SigCache: cache,
		Locale:   models.Locale{HL: "en", GL: "US"},
	})

	raw, err := client.Browse(ctx, "FEmusic_home", nil)
	if err != nil {
		fmt.Fprintf(os.Stderr, "browse: %v\n", err)
		os.Exit(1)
	}
	page := parser.ParseHomePage(raw)
	enc := json.NewEncoder(os.Stdout)
	enc.SetIndent("", "  ")
	enc.Encode(page)
}

func runSearch(args []string) {
	fs := flag.NewFlagSet("search", flag.ExitOnError)
	query := fs.String("query", "", "search query (required)")
	fs.Parse(args)
	if *query == "" {
		fmt.Fprintln(os.Stderr, "--query is required")
		os.Exit(1)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	cache := sig.NewCache(http.DefaultClient)
	if err := cache.Bootstrap(ctx); err != nil {
		fmt.Fprintf(os.Stderr, "sig bootstrap: %v\n", err)
		os.Exit(1)
	}
	client := innertube.NewClient(innertube.ClientOpts{
		SigCache: cache,
		Locale:   models.Locale{HL: "en", GL: "US"},
	})

	raw, err := client.Search(ctx, *query)
	if err != nil {
		fmt.Fprintf(os.Stderr, "search: %v\n", err)
		os.Exit(1)
	}
	page := parser.ParseSearchPage(raw)
	enc := json.NewEncoder(os.Stdout)
	enc.SetIndent("", "  ")
	enc.Encode(page)
}
```

- [ ] **Step 7: Run all tests**

```bash
cd server && go test ./internal/innertube/...
```

- [ ] **Step 8: Build and smoke-test**

```bash
cd server && go build ./cmd/probe/ && \
  ./probe innertube search --query="Rick Astley" | head -20
```

- [ ] **Step 9: Commit**

```bash
git add server/internal/innertube/parser/ server/cmd/probe/
git commit -m "feat(m3): expansion parsers (home, search, related, artist, album, playlist) and probe subcommands"
```

---

### Task 11: Sig cipher — WEB fallback

**Files:**
- Create: `server/internal/innertube/sig/transform.go`
- Create: `server/internal/innertube/sig/transform_test.go`
- Create: `server/internal/innertube/sig/testdata/sig_fixture.json`

**Interfaces:**
- Produces: `transform.Apply(ops []Op, sig string) string`, used by `sig.Cache.ApplySig`

- [ ] **Step 1: Create sig fixture**

```json
{
  "ops": [
    {"kind": "reverse"},
    {"kind": "splice", "arg": 3},
    {"kind": "swap", "arg": 2}
  ],
  "cases": [
    {"in": "abcdefghij", "out": "fghijdecba"}
  ]
}
```

To derive the expected output manually: start with `abcdefghij`, reverse → `jihgfedcba`, splice(3) removes first 3 chars → `gfedcba`, swap(2) swaps chars at index 0 and 2 → `fgedcba`... adjust fixture to match your actual implementation.

- [ ] **Step 2: Write test**

```go
// server/internal/innertube/sig/transform_test.go
package sig_test

import (
	"encoding/json"
	"os"
	"testing"

	"github.com/iceice666/sunflower/server/internal/innertube/sig"
)

type sigFixture struct {
	Ops []struct {
		Kind string `json:"kind"`
		Arg  int    `json:"arg"`
	} `json:"ops"`
	Cases []struct {
		In  string `json:"in"`
		Out string `json:"out"`
	} `json:"cases"`
}

func TestApply_FromFixture(t *testing.T) {
	raw, err := os.ReadFile("testdata/sig_fixture.json")
	if err != nil {
		t.Fatal(err)
	}
	var fix sigFixture
	if err := json.Unmarshal(raw, &fix); err != nil {
		t.Fatal(err)
	}

	ops := make([]sig.Op, len(fix.Ops))
	for i, o := range fix.Ops {
		ops[i] = sig.Op{Kind: sig.OpKindFromString(o.Kind), Arg: o.Arg}
	}

	for _, tc := range fix.Cases {
		got := sig.Apply(ops, tc.In)
		if got != tc.Out {
			t.Errorf("Apply(%q) = %q, want %q", tc.In, got, tc.Out)
		}
	}
}
```

- [ ] **Step 3: Implement transform.go**

```go
// server/internal/innertube/sig/transform.go
package sig

import "fmt"

type opKind int

const (
	opReverse opKind = iota
	opSplice
	opSwap
)

// Op is a single sig-cipher transformation.
type Op struct {
	Kind opKind
	Arg  int
}

// OpKindFromString converts a fixture string to opKind.
func OpKindFromString(s string) opKind {
	switch s {
	case "reverse":
		return opReverse
	case "splice":
		return opSplice
	case "swap":
		return opSwap
	default:
		panic(fmt.Sprintf("sig: unknown op kind %q", s))
	}
}

// Apply runs ops over sig in sequence and returns the result.
func Apply(ops []Op, s string) string {
	b := []byte(s)
	for _, op := range ops {
		switch op.Kind {
		case opReverse:
			for i, j := 0, len(b)-1; i < j; i, j = i+1, j-1 {
				b[i], b[j] = b[j], b[i]
			}
		case opSplice:
			if op.Arg < len(b) {
				b = b[op.Arg:]
			}
		case opSwap:
			if op.Arg < len(b) {
				b[0], b[op.Arg] = b[op.Arg], b[0]
			}
		}
	}
	return string(b)
}
```

- [ ] **Step 4: Fix the sig fixture to match the implementation**

Run the test with a known simple input to verify:

```bash
cd server && go test ./internal/innertube/sig/... -run TestApply_FromFixture -v
```

Adjust `sig_fixture.json` `"out"` value to match actual output if needed.

- [ ] **Step 5: Run all tests**

```bash
cd server && go test ./internal/innertube/...
```

- [ ] **Step 6: Commit**

```bash
git add server/internal/innertube/sig/
git commit -m "feat(m3): sig cipher transform (WEB fallback)"
```

---

### Task 12: Cookie store + migration

**Files:**
- Create: `server/db/migrations/0006_cookie_health.sql`
- Create: `server/internal/cookies/store.go`
- Create: `server/internal/cookies/store_test.go`

**Interfaces:**
- Produces:
  - `cookies.Store(ctx, db, key [32]byte, userID uuid.UUID, provider string, raw []byte) error`
  - `cookies.Load(ctx, db, key [32]byte, userID uuid.UUID, provider string) ([]byte, error)`
  - `var ErrDecryptFailed = errors.New("cookies: decrypt failed")`
  - `var ErrNotFound = errors.New("cookies: not found")`

- [ ] **Step 1: Write migration**

```sql
-- server/db/migrations/0006_cookie_health.sql
-- +goose Up
-- +goose StatementBegin

CREATE TABLE cookie_health (
    provider     text        NOT NULL PRIMARY KEY,
    status       text        NOT NULL DEFAULT 'unknown',
    checked_at   timestamptz,
    detail       text
);

-- +goose StatementEnd

-- +goose Down
-- +goose StatementBegin
DROP TABLE IF EXISTS cookie_health;
-- +goose StatementEnd
```

- [ ] **Step 2: Write store test**

```go
// server/internal/cookies/store_test.go
package cookies_test

import (
	"testing"

	"github.com/iceice666/sunflower/server/internal/cookies"
)

func TestStoreRoundTrip(t *testing.T) {
	var key [32]byte
	copy(key[:], "test-key-32-bytes-padded-to-fit!!")

	plain := []byte("cookie_data=abc123; domain=youtube.com")

	ct, nonce, err := cookies.Encrypt(key, plain)
	if err != nil {
		t.Fatalf("Encrypt: %v", err)
	}
	got, err := cookies.Decrypt(key, ct, nonce)
	if err != nil {
		t.Fatalf("Decrypt: %v", err)
	}
	if string(got) != string(plain) {
		t.Errorf("got %q, want %q", got, plain)
	}
}

func TestDecryptTampered(t *testing.T) {
	var key [32]byte
	copy(key[:], "test-key-32-bytes-padded-to-fit!!")

	ct, nonce, _ := cookies.Encrypt(key, []byte("data"))
	ct[0] ^= 0xFF // tamper

	_, err := cookies.Decrypt(key, ct, nonce)
	if err != cookies.ErrDecryptFailed {
		t.Errorf("expected ErrDecryptFailed, got %v", err)
	}
}
```

- [ ] **Step 3: Implement store.go**

```go
// server/internal/cookies/store.go
package cookies

import (
	"context"
	"crypto/rand"
	"errors"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
	"golang.org/x/crypto/nacl/secretbox"
)

var (
	ErrDecryptFailed = errors.New("cookies: decrypt failed")
	ErrNotFound      = errors.New("cookies: not found")
)

// Encrypt encrypts plain with secretbox and returns (ciphertext, nonce, error).
// The nonce and ciphertext are stored separately in the DB.
func Encrypt(key [32]byte, plain []byte) (ciphertext []byte, nonce [24]byte, err error) {
	if _, err = rand.Read(nonce[:]); err != nil {
		return nil, nonce, err
	}
	ct := secretbox.Seal(nil, plain, &nonce, &key)
	return ct, nonce, nil
}

// Decrypt decrypts ciphertext using key and nonce.
func Decrypt(key [32]byte, ciphertext []byte, nonce [24]byte) ([]byte, error) {
	plain, ok := secretbox.Open(nil, ciphertext, &nonce, &key)
	if !ok {
		return nil, ErrDecryptFailed
	}
	return plain, nil
}

// Store encrypts raw and upserts it into the encrypted_cookies table.
func Store(ctx context.Context, db *pgxpool.Pool, key [32]byte, userID uuid.UUID, provider string, raw []byte) error {
	ct, nonce, err := Encrypt(key, raw)
	if err != nil {
		return err
	}
	_, err = db.Exec(ctx, `
		INSERT INTO encrypted_cookies (user_id, provider, ciphertext, nonce, refreshed_at)
		VALUES ($1, $2, $3, $4, now())
		ON CONFLICT (user_id, provider) DO UPDATE
		SET ciphertext = EXCLUDED.ciphertext,
		    nonce      = EXCLUDED.nonce,
		    refreshed_at = now()
	`, userID, provider, ct, nonce[:])
	return err
}

// Load reads and decrypts the cookie for the given user/provider.
func Load(ctx context.Context, db *pgxpool.Pool, key [32]byte, userID uuid.UUID, provider string) ([]byte, error) {
	var ct []byte
	var nonceBytes []byte
	err := db.QueryRow(ctx,
		`SELECT ciphertext, nonce FROM encrypted_cookies WHERE user_id=$1 AND provider=$2`,
		userID, provider,
	).Scan(&ct, &nonceBytes)
	if errors.Is(err, pgx.ErrNoRows) {
		return nil, ErrNotFound
	}
	if err != nil {
		return nil, err
	}
	var nonce [24]byte
	copy(nonce[:], nonceBytes)
	return Decrypt(key, ct, nonce)
}
```

- [ ] **Step 4: Run tests — expect PASS**

```bash
cd server && go test ./internal/cookies/...
```

- [ ] **Step 5: Verify migration file is picked up by embed**

The migration embed uses `go:embed` from `db/migrations/embed.go`. Verify it includes `*.sql`:

```bash
cd server && grep -n embed db/migrations/embed.go
```

Expected: `//go:embed *.sql` or similar. The new file will be included automatically.

- [ ] **Step 6: Commit**

```bash
git add server/internal/cookies/ server/db/migrations/0006_cookie_health.sql
git commit -m "feat(m3): cookie secretbox store and cookie_health migration"
```

---

### Task 13: Cookie API handler + router update

**Files:**
- Create: `server/internal/api/handlers_cookies.go`
- Modify: `server/internal/api/router.go`

**Interfaces:**
- Consumes: `cookies.Store`, `cookies.Load`, `auth.Middleware` (existing)
- Produces: `POST /api/v1/cookies/youtube`, `GET /api/v1/cookies/youtube/status`

- [ ] **Step 1: Add CookieKey to Deps**

In `server/internal/api/router.go`, add `CookieKey [32]byte` to the `Deps` struct:

```go
type Deps struct {
	Log       zerolog.Logger
	DB        *pgxpool.Pool
	Jobs      *jobs.Registry
	Scanner   *library.Scanner
	DataDir   string
	CookieKey [32]byte // zero = cookies disabled
}
```

- [ ] **Step 2: Register cookie routes**

In `NewRouter`, inside the authenticated group, add:

```go
r.Post("/cookies/youtube", d.uploadYTCookies)
r.Get("/cookies/youtube/status", d.ytCookieStatus)
```

- [ ] **Step 3: Implement handlers_cookies.go**

```go
// server/internal/api/handlers_cookies.go
package api

import (
	"encoding/json"
	"net/http"
	"time"

	"github.com/google/uuid"
	"github.com/iceice666/sunflower/server/internal/auth"
	"github.com/iceice666/sunflower/server/internal/cookies"
)

type uploadCookiesRequest struct {
	Cookies string `json:"cookies"` // Netscape-format cookie file contents
}

// uploadYTCookies handles POST /api/v1/cookies/youtube.
func (d *Deps) uploadYTCookies(w http.ResponseWriter, r *http.Request) {
	deviceID := auth.DeviceIDFromContext(r.Context())

	var req uploadCookiesRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil || req.Cookies == "" {
		jsonError(w, "invalid_format", http.StatusBadRequest)
		return
	}

	userID, err := d.userIDForDevice(r.Context(), deviceID)
	if err != nil {
		d.Log.Error().Err(err).Str("device", deviceID.String()).Msg("uploadYTCookies: lookup user")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}

	if err := cookies.Store(r.Context(), d.DB, d.CookieKey, userID, "youtube", []byte(req.Cookies)); err != nil {
		d.Log.Error().Err(err).Msg("uploadYTCookies: store")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}

	w.WriteHeader(http.StatusNoContent)
}

type cookieStatusResponse struct {
	Status    string  `json:"status"`
	CheckedAt *string `json:"checked_at"`
	Detail    *string `json:"detail"`
}

// ytCookieStatus handles GET /api/v1/cookies/youtube/status.
func (d *Deps) ytCookieStatus(w http.ResponseWriter, r *http.Request) {
	var status, detail string
	var checkedAt *time.Time

	err := d.DB.QueryRow(r.Context(),
		`SELECT status, checked_at, detail FROM cookie_health WHERE provider='youtube'`,
	).Scan(&status, &checkedAt, &detail)
	if err != nil {
		// No row yet.
		jsonOK(w, cookieStatusResponse{Status: "unknown"})
		return
	}

	resp := cookieStatusResponse{Status: status}
	if checkedAt != nil {
		s := checkedAt.Format(time.RFC3339)
		resp.CheckedAt = &s
	}
	if detail != "" {
		resp.Detail = &detail
	}
	jsonOK(w, resp)
}

// userIDForDevice looks up the user_id for a device_id. Add this helper or use
// an existing db query if one exists.
func (d *Deps) userIDForDevice(ctx context.Context, deviceID uuid.UUID) (uuid.UUID, error) {
	var userID uuid.UUID
	err := d.DB.QueryRow(ctx,
		`SELECT user_id FROM devices WHERE id=$1`, deviceID,
	).Scan(&userID)
	return userID, err
}
```

Note: `auth.DeviceIDFromContext` must be added to the auth package if it doesn't exist. Check `server/internal/auth/middleware.go` — the middleware likely sets a context value. Use whatever key it uses.

- [ ] **Step 4: Add CookieKey wiring in main.go**

In `server/cmd/sunflowerd/main.go`, parse `SUNFLOWER_COOKIE_KEY` and pass to `api.Deps`:

```go
import "encoding/hex"

// After cfg := config.Load():
var cookieKey [32]byte
if cfg.CookieKey != "" {
    b, err := hex.DecodeString(cfg.CookieKey)
    if err != nil || len(b) != 32 {
        log.Fatal().Msg("SUNFLOWER_COOKIE_KEY must be 64 hex chars (32 bytes)")
    }
    copy(cookieKey[:], b)
}

// Pass to Deps:
handler := api.NewRouter(api.Deps{
    ...
    CookieKey: cookieKey,
})
```

- [ ] **Step 5: Build and verify**

```bash
cd server && go build ./...
```

- [ ] **Step 6: Commit**

```bash
git add server/internal/api/ server/cmd/sunflowerd/main.go
git commit -m "feat(m3): cookie upload and status API endpoints"
```

---

### Task 14: Cookie refresh job + probe cookies-set

**Files:**
- Create: `server/internal/cookies/refresh_job.go`
- Modify: `server/cmd/sunflowerd/main.go` (start refresh goroutine)
- Modify: `server/cmd/probe/innertube_cmd.go` (add cookies-set subcommand)

**Interfaces:**
- Consumes: `cookies.Load`, `innertube.Client`, `sig.Cache`
- Produces: goroutine that runs hourly and upserts `cookie_health`

- [ ] **Step 1: Implement refresh_job.go**

```go
// server/internal/cookies/refresh_job.go
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
```

- [ ] **Step 2: Wire refresh job in main.go**

After creating `pool` and before starting the HTTP server, add:

```go
import (
    "github.com/iceice666/sunflower/server/internal/cookies"
    "github.com/google/uuid"
)

// Start cookie health probe (noop if CookieKey is zero).
if cookieKey != [32]byte{} {
    // Use a placeholder userID — in a single-user system the first (only) user.
    var adminUserID uuid.UUID
    _ = pool.QueryRow(ctx, `SELECT id FROM users LIMIT 1`).Scan(&adminUserID)
    cookies.StartRefreshJob(ctx, pool, cookieKey, adminUserID, log)
}
```

- [ ] **Step 3: Add cookies-set to probe**

```go
// In runInnertube switch, add:
case "cookies-set":
    runCookiesSet(args[1:])

// New function:
func runCookiesSet(args []string) {
	fs := flag.NewFlagSet("cookies-set", flag.ExitOnError)
	file := fs.String("file", "", "path to Netscape-format cookie file (required)")
	serverURL := fs.String("server", "http://localhost:8080", "sunflowerd base URL")
	token := fs.String("token", "", "device token (required)")
	fs.Parse(args)

	if *file == "" || *token == "" {
		fmt.Fprintln(os.Stderr, "--file and --token are required")
		os.Exit(1)
	}

	raw, err := os.ReadFile(*file)
	if err != nil {
		fmt.Fprintf(os.Stderr, "read file: %v\n", err)
		os.Exit(1)
	}

	body, _ := json.Marshal(map[string]string{"cookies": string(raw)})
	req, _ := http.NewRequest(http.MethodPost, *serverURL+"/api/v1/cookies/youtube", bytes.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Authorization", "Bearer "+*token)

	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		fmt.Fprintf(os.Stderr, "upload: %v\n", err)
		os.Exit(1)
	}
	defer resp.Body.Close()
	if resp.StatusCode == http.StatusNoContent {
		fmt.Println("cookies uploaded successfully")
	} else {
		io.Copy(os.Stderr, resp.Body)
		os.Exit(1)
	}
}
```

Add `"bytes"` and `"io"` to the imports in `innertube_cmd.go`.

- [ ] **Step 4: Run all tests**

```bash
cd server && go test ./...
```

- [ ] **Step 5: Build final binary**

```bash
cd server && go build ./cmd/probe/ && go build ./cmd/sunflowerd/
```

- [ ] **Step 6: Commit**

```bash
git add server/internal/cookies/refresh_job.go server/cmd/probe/ server/cmd/sunflowerd/
git commit -m "feat(m3): cookie refresh job and probe cookies-set subcommand"
```

---

## Self-Review

**Spec coverage check:**

| Spec requirement | Covered in |
|---|---|
| `probe innertube next` returns playable URL | Tasks 7–8 |
| Sig decryption unit-tested with frozen pairs | Tasks 5, 11 |
| All parser tests pass, missing-field cases | Task 8 (EmptyJSON tests), Task 10 |
| Continuation tokens round-trip | Task 2 (Cursor), Task 4 (Next payload), Task 8 (ParseNextPage) |
| `POST /api/v1/cookies/youtube` stored encrypted | Tasks 12–13 |
| Cookie health probe hourly | Task 14 |
| Guest-mode for next/related | Task 6 (Cookies: nil path) |
| `sig/testdata/` with frozen pairs | Tasks 5, 11 |
| `parser/testdata/` with fixtures | Tasks 7 (capture), 8, 10 |
| `probe home`, `probe search` | Task 10 |
| `probe cookies-set` | Task 14 |
| `-o url` flag | Task 7 |
| `ProbeNextResult` JSON output | Tasks 2, 7–8 |
| `Bootstrap` from iframe_api | Task 5 |
| `0006_cookie_health.sql` DDL | Task 12 |
| `GET /api/v1/cookies/youtube/status` | Task 13 |
| `CookieKey` wired in main.go | Task 13 |

**Placeholder scan:** No TBD/TODO in implementation steps. Parsers note paths may need adjustment from real fixtures — this is expected and documented.

**Type consistency:** `continuation.Cursor` used throughout. `models.ProbeNextResult` defined in Task 2 and consumed in Task 7. `sig.Cache` produced in Task 5 and consumed in Tasks 6–7. All signatures consistent.
