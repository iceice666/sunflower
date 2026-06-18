# M3 InnerTube Client — Design Spec

**Date:** 2026-06-18
**Status:** approved (v4 — third reviewer pass applied)
**Demo target:** `probe innertube next --video-id=<id>` returns a fresh, playable stream URL without any external service.

---

## Approach

Vertical slice (Approach A): build `probe innertube next` end-to-end first, use
live YT responses to capture parser fixtures, then expand to remaining surfaces.
This front-loads the highest-risk piece (sig/n-param decryption) and generates
authentic fixtures organically.

---

## 1. Package layout

```
server/internal/innertube/
  models/models.go          public structs (PlayerResponse, NextPage, SongItem…)
  context.go                ANDROID_MUSIC / WEB_REMIX payload context builders
  client.go                 HTTP client: POST to InnerTube, cookie injection, retry
  sig/
    base_js.go              fetch + cache base.js by player-JS-ID hash
    transform.go            sig-cipher op list (reverse/splice/swap) for WEB context
    nsig.go                 n-param decryption via embedded goja JS engine
    testdata/               frozen base.js snippets + (n_in, n_out) pairs for unit tests
  payloads/
    player.go               /youtubei/v1/player request body
    next.go                 /youtubei/v1/next
    browse.go               /youtubei/v1/browse
    search.go               /youtubei/v1/search
  parser/
    yt_item.go              SongItem, AlbumItem, ArtistItem, PlaylistItem helpers
    next_page.go            WatchEndpoint + related + continuation
    home_page.go
    related_page.go
    artist_page.go
    album_page.go
    playlist_page.go
    search_page.go          SearchPage with song/album/artist results
    testdata/               fixture JSON captured from live calls, one file per surface
  continuation/
    cursor.go               opaque []byte token, round-trip only
server/internal/cookies/
  store.go                  secretbox encrypt/decrypt, reads two-column schema
  refresh_job.go            hourly health-probe call
server/internal/api/
  handlers_cookies.go       POST /api/v1/cookies/youtube + GET /api/v1/cookies/youtube/status
server/db/migrations/
  0006_cookie_health.sql    adds cookie_health table (single row per provider)
server/cmd/probe/
  main.go
  innertube_cmd.go          subcommands: next, home, search, cookies-set
```

### Vertical-slice build order

1. `models/` — all public structs (no deps)
2. `context.go` — ANDROID_MUSIC context builder
3. `payloads/player.go` — POST body for `/player`
4. `client.go` — bare HTTP client, no cookies yet
5. `sig/base_js.go` + `sig/nsig.go` — Bootstrap (iframe_api), n-param decryption; add `goja` dep
6. `payloads/player.go` + `payloads/next.go` + bare `client.go` — call `Client.Player` then `Client.Next` against a real video; save live responses as `testdata/player_response.json` and `testdata/next_response.json`; capture base.js nsig fixture
7. `parser/next_page.go` + `parser/yt_item.go` — parse captured fixtures; `probe innertube next` emits `ProbeNextResult` JSON and `-o url` emits `CurrentURL`; **demo target met**
8. Expansion pass (in order): `payloads/browse.go`, `payloads/search.go`; then `parser/home_page.go`, `parser/related_page.go`, `parser/artist_page.go`, `parser/album_page.go`, `parser/playlist_page.go`, `parser/search_page.go`; plus `probe home` and `probe search` subcommands; `sig/transform.go` (WEB cipher fallback)
9. `cookies/store.go` + `0006_cookie_health.sql` migration + `handlers_cookies.go` + `refresh_job.go` + `probe cookies-set`

---

## 2. Models

```go
// models/models.go

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
    PlayerJsURL string     // absolute base.js URL; see §4b for source
    Stream      StreamURL  // best audio format after n-param decode
    AllStreams   []StreamURL
}

// HomePage is populated by ParseHomePage (expansion pass).
// Minimal definition here; field list is finalised during the expansion pass
// once live fixtures are captured.
type HomePage struct {
    Sections []HomeSection
    Chips    []string // mood/genre filter chips; empty in guest mode
}

type HomeSection struct {
    Title string
    Items []any // SongItem | AlbumItem | PlaylistItem
}

type NextPage struct {
    Current      SongItem
    Related      []SongItem
    Continuation Cursor // zero → no more pages
}

type SearchPage struct {
    Songs    []SongItem
    Albums   []AlbumItem
    Artists  []ArtistItem
    Continuation Cursor
}

// ProbeNextResult is the output struct for `probe innertube next`.
// It merges a Client.Player call (stream URL) and a Client.Next call (related items).
// json tags match the milestone demo output (snake_case).
type ProbeNextResult struct {
    CurrentURL   string     `json:"current_url"`
    ExpiresAt    time.Time  `json:"expires_at"`
    Itag         int        `json:"itag"`
    NextItems    []SongItem `json:"next_items"`
    Continuation Cursor     `json:"continuation,omitempty"`
}

// Locale carries the language/region hint sent with every InnerTube request.
type Locale struct {
    HL string // e.g. "en"
    GL string // e.g. "US"
}
```

**Locating base.js (critical path for n-param decryption):**

ANDROID_MUSIC player responses do not reliably contain a `jsUrl` field. The
base.js URL is therefore sourced via a separate lightweight bootstrap step:

1. On first call (or after cache expiry), `sig.Cache.Bootstrap(ctx)` fetches
   `https://www.youtube.com/iframe_api`, follows the redirect to a URL of the
   form `https://www.youtube.com/s/player/<hash>/www-widgetapi.vflset/www-widgetapi.js`,
   and extracts `<hash>` to construct the canonical base.js URL:
   `https://www.youtube.com/s/player/<hash>/player_ias.vflset/en_US/base.js`
2. This base.js URL is cached for 6 hours. The `PlayerResponse.PlayerJsURL`
   field is populated by `sig.Cache` during `Client.Player` if a newer hash is
   observed in the player response; otherwise it holds the bootstrapped URL.
3. `DecodeN` always receives the explicit `playerJsURL` from `PlayerResponse`;
   there is no derivation from URL query parameters.

If `Bootstrap` fails (network error), `sig.Cache` returns `ErrNoPlayerJs` and
the caller should surface a degraded-mode warning.

---

## 3. HTTP client

```go
// client.go

// WEB_REMIX is used for surfaces that require the full YT Music web experience
// (e.g. browse endpoints for artist/album pages). ANDROID_MUSIC is used for
// player and next — it returns plain stream URLs (no signatureCipher). The
// Client chooses context per-method; callers do not select it directly.
type Client struct {
    http       *http.Client
    sig        *sig.Cache
    cookies    func() []*http.Cookie // nil = guest mode
    cookieSink func([]*http.Cookie)  // captures Set-Cookie rotations; may be nil
    locale     Locale
}

func (c *Client) Player(ctx context.Context, videoID string) (models.PlayerResponse, error)
func (c *Client) Next(ctx context.Context, videoID string, cont continuation.Cursor) (models.NextPage, error)
func (c *Client) Browse(ctx context.Context, browseID string, cont continuation.Cursor) (json.RawMessage, error)
func (c *Client) Search(ctx context.Context, query string) (json.RawMessage, error)
```

The client never parses; parsers never make HTTP calls. `Browse` and `Search`
return raw JSON — the caller passes it to the appropriate parser.

On every response the client inspects `Set-Cookie` and forwards new cookies to
`cookieSink` if set, enabling the cookie store to capture YT session rotations.

The client retries once on 5xx before returning an error.

---

## 4. Sig decryption (`sig/`)

Two separate sub-problems:

### 4a. Sig cipher (`transform.go`)

Applies when a WEB_REMIX context response returns `signatureCipher` instead of
a plain `url`. ANDROID_MUSIC returns plain URLs, so this path is only exercised
by WEB_REMIX browse/search surfaces (expansion pass, step 8).

- Fetch `base.js` using the URL from `PlayerResponse.PlayerJsURL`
- Regex-extract the sig transform function; parse its body into `[]Op`
- Three op kinds: `Reverse`, `Splice(n)`, `Swap(n)`
- Apply ops to the ciphered sig string in sequence

```go
type Op struct { Kind opKind; Arg int }
func Apply(ops []Op, sig string) string // pure function, no I/O
```

### 4b. N-param decryption (`nsig.go`)

Every stream URL (plain or ciphered) contains `?n=<token>`. YouTube throttles
if `n` is not replaced with the decoded value. The decode logic is an obfuscated
JS function in `base.js` that rotates frequently.

**Implementation: `goja` JS engine.** Extract the JS function text from
`base.js` by regex, compile it once per base.js version with `goja.Compile`,
call it per URL with a `goja.Runtime`. This is robust against function-body
changes; only the extraction regex needs updating if YT renames the function.

Add the dependency from `server/`:
```
go get github.com/dop251/goja@latest
```
Pin the resulting pseudo-version in `go.mod`. `goja` is pure Go, no CGo.

```go
// sig.Cache holds parsed ops + compiled goja program per player-JS-ID.
type Cache struct { mu sync.RWMutex; entries map[string]*entry }

// Bootstrap fetches https://www.youtube.com/iframe_api, extracts the current
// player hash, and pre-warms the base.js cache. Call once at startup; the
// cache refreshes automatically on TTL expiry or 403 burst.
func (c *Cache) Bootstrap(ctx context.Context) error

// DecodeN replaces the ?n= query param in rawURL with the decoded value.
// playerJsURL is passed explicitly from PlayerResponse.PlayerJsURL;
// if empty, the most recently bootstrapped base.js entry is used.
func (c *Cache) DecodeN(ctx context.Context, rawURL, playerJsURL string) (string, error)
func (c *Cache) ApplySig(ctx context.Context, cipher, playerJsURL string) (string, error)
```

`playerJsURL` is always passed explicitly (sourced from `PlayerResponse.PlayerJsURL`
or the bootstrapped URL). There is no derivation from stream URL query parameters.

### 403 sustained-burst handling

The `Cache` tracks consecutive 403s per player-JS-ID. After 3 in a 60s window
it evicts the entry and re-fetches `base.js` on the next call. If 403s persist
after re-fetch, it returns `ErrSigInvalidated` so the caller can alert.

### File naming

The implementation file is `sig/nsig.go` (not `cipher.go`). `cipher.go` was
a placeholder name in the milestone; `nsig.go` is the canonical name used
throughout this spec.

---

## 5. Parsers

### Contract

```go
// Every Parse* function: takes raw json.RawMessage, returns typed struct, never errors.
func ParsePlayerResponse(raw json.RawMessage) models.PlayerResponse
func ParseNextPage(raw json.RawMessage) models.NextPage
func ParseHomePage(raw json.RawMessage) models.HomePage
func ParseSearchPage(raw json.RawMessage) models.SearchPage
// … etc.
```

Missing fields → zero value. Unknown renderer kinds → `zerolog` debug log, then skip.

### Tree-walking helpers

YT's response JSON is deeply nested. Parsers use targeted helpers rather than
one giant unmarshal struct:

```go
func getString(m map[string]any, path ...string) string
func getArray(m map[string]any, path ...string) []any
func getInt(m map[string]any, path ...string) int
```

### Fixtures

Each parser has a corresponding file under `parser/testdata/`. Fixtures are
captured once from live YT calls via the probe CLI, committed, and never
regenerated in CI. When YT drifts, the parser is updated and the fixture
re-captured manually.

---

## 6. Continuation

```go
// continuation/cursor.go
type Cursor []byte // opaque; extracted by parsers, posted back verbatim
func (c Cursor) IsZero() bool
```

Parsers extract the raw continuation token string from the response JSON and
store it as `[]byte`. The client casts a non-zero `Cursor` directly to `string`
and sets it as the POST body's `continuation` field — no base64 encoding or
decoding occurs. The token is treated as fully opaque and is never inspected.

---

## 7. Cookie store

### Encryption (`cookies/store.go`)

Uses `golang.org/x/crypto/nacl/secretbox` — same `crypto_secretbox_xsalsa20poly1305`
primitive as libsodium, pure Go, no CGo. The existing `encrypted_cookies` table
(migration `0004_sync.sql`) has two separate columns: `ciphertext bytea` and
`nonce bytea`. The implementation must write them separately:

```go
// Store encrypts raw with secretbox, writes ciphertext and nonce as separate columns.
func Store(ctx context.Context, db *pgxpool.Pool, key [32]byte, userID uuid.UUID, provider string, raw []byte) error
// Load reads ciphertext+nonce, decrypts, returns plaintext.
func Load(ctx context.Context, db *pgxpool.Pool, key [32]byte, userID uuid.UUID, provider string) ([]byte, error)
```

Nonce is random 24 bytes, stored in the `nonce` column. The ciphertext column
holds only the secretbox output (no nonce prefix). Tampered ciphertext returns
`ErrDecryptFailed`; missing row returns `ErrNotFound`.

### `cookie_health` table (migration `0006_cookie_health.sql`)

```sql
CREATE TABLE cookie_health (
    provider     text        NOT NULL PRIMARY KEY,
    status       text        NOT NULL DEFAULT 'unknown', -- 'ok' | 'degraded' | 'unknown'
    checked_at   timestamptz,
    detail       text        -- last error message if degraded
);
```

Single row per provider, upserted on each probe run.

### Health probe (`cookies/refresh_job.go`)

Runs hourly: calls `Client.Next` with the stored cookies against a
known-stable video ID (`dQw4w9WgXcQ`), upserts `cookie_health` with
`status='ok'` or `status='degraded'` + error detail.

### API endpoints (`internal/api/handlers_cookies.go`)

**Upload cookies:**
```
POST /api/v1/cookies/youtube
Authorization: Bearer <device-token>
Content-Type: application/json
{ "cookies": "<Netscape-format cookie file contents as a string>" }

→ 204 No Content   (success; server never echoes cookie data back)
→ 400 { "error": "invalid_format" }
→ 401 { "error": "unauthorized" }
```

**Status:**
```
GET /api/v1/cookies/youtube/status
Authorization: Bearer <device-token>

→ 200 { "status": "ok"|"degraded"|"unknown", "checked_at": "<ISO8601>|null", "detail": "<error string>|null" }
```

Both routes are protected by the existing auth middleware. Registration in
`router.go` under the authenticated group.

---

## 8. Probe CLI (`cmd/probe/innertube_cmd.go`)

Subcommands and output flags:

```
probe innertube next --video-id=<id> [--cookies=<path>] [-o json|url]
probe innertube home [--cookies=<path>]
probe innertube search --query=<q> [--cookies=<path>]
probe innertube cookies-set --file=<netscape-cookie-file>
```

`probe innertube next` calls `Client.Player` (for the stream URL) and
`Client.Next` (for related items and continuation) sequentially, then merges
the results into a `ProbeNextResult`. `-o url` prints only `CurrentURL`.
`-o json` (default) prints the full `ProbeNextResult` as JSON.

`probe innertube home` is part of the expansion pass (build order step 8).

---

## 9. Error handling summary

| Tier | Behaviour |
|---|---|
| Network / non-2xx | Wrapped error returned to caller; client retries once on 5xx |
| 403 sustained burst | Evict sig cache entry, re-fetch base.js; `ErrSigInvalidated` if persists |
| Parser missing field | Zero value returned; unknown renderer kind logged at debug, skipped |
| Cookie decrypt failure | `ErrDecryptFailed` returned; health probe marks status "degraded" |
| Cookie missing | `ErrNotFound`; client falls back to guest mode |

---

## 10. Testing

| Layer | Method |
|---|---|
| `sig/transform.go` | Unit tests: frozen `(ops, input) → expected` pairs; fixtures in `sig/testdata/` |
| `sig/nsig.go` | Unit tests: frozen base.js snippet + known `(n_in, n_out)` pairs; fixtures in `sig/testdata/` |
| `parser/*.go` | Table-driven tests against `parser/testdata/` fixtures; normal + missing-field cases per surface |
| `cookies/store.go` | Unit test: encrypt → decrypt round-trip; tampered ciphertext returns `ErrDecryptFailed` |
| `probe innertube next` | Manual end-to-end only (requires live YT); not in CI |

No test containers needed for M3. Cookie DB interaction is covered by the
integration test harness introduced in M1 when the cookie endpoint is wired in.

---

## 11. Out of scope for M3

- Personalized home/explore surface assembly (M5)
- Stream proxy fallback (M4)
- `internal/recs` integration
