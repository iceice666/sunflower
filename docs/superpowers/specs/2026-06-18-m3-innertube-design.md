# M3 InnerTube Client — Design Spec

**Date:** 2026-06-18
**Status:** approved
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
    testdata/               fixture JSON captured from live calls
  continuation/
    cursor.go               opaque []byte token, round-trip only
server/internal/cookies/
  store.go                  secretbox encrypt/decrypt
  refresh_job.go            hourly health-probe call
server/cmd/probe/
  main.go
  innertube_cmd.go          subcommands: next, home, search, cookies-set
```

### Vertical-slice build order

1. `models/` — all public structs (no deps)
2. `context.go` — ANDROID_MUSIC context builder
3. `payloads/player.go` — POST body for `/player`
4. `client.go` — bare HTTP client, no cookies yet
5. `sig/base_js.go` + `sig/nsig.go` — n-param decryption
6. Wire `probe innertube next` → make real YT call → save response as `testdata/player_with_sig.json`
7. `parser/next_page.go` + `parser/yt_item.go` — parse captured fixture; **demo target met**
8. Expand: remaining payloads + parsers with fixtures from real calls
9. `cookies/store.go` + `refresh_job.go` + `probe cookies-set` subcommand

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

// AlbumItem, ArtistItem, PlaylistItem follow the same optional-field pattern.

type PlayerResponse struct {
    VideoID   string
    Stream    StreamURL   // best audio format after n-param decode
    AllStream []StreamURL // all formats; caller selects quality
}

type NextPage struct {
    Current      SongItem
    Related      []SongItem
    Continuation Cursor // zero → no more pages
}
```

Zero values everywhere — no pointer fields, no error returns from parsers.

---

## 3. HTTP client

```go
// client.go

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

Applies when a WEB context response returns `signatureCipher` instead of a
plain `url`. ANDROID_MUSIC returns plain URLs, so this is a WEB-fallback path.

- Fetch `base.js` from `https://www.youtube.com/s/player/<player_js_id>/…/base.js`
- Regex-extract the sig transform function; parse its body into `[]Op`
- Three op kinds: `Reverse`, `Splice(n)`, `Swap(n)`
- Apply ops to the ciphered sig string in sequence

```go
type Op struct { Kind opKind; Arg int }
func Apply(ops []Op, sig string) string // pure function
```

### 4b. N-param decryption (`nsig.go`)

Every stream URL (plain or ciphered) contains `?n=<token>`. YouTube throttles
if `n` is not replaced with the decoded value. The decode logic is an obfuscated
JS function in `base.js` that rotates frequently.

**Implementation: `goja` JS engine.** Extract the JS function text from
`base.js` by regex, compile it once per base.js version, execute it with
`goja.RunString`. This is robust against function-body changes; only the
extraction regex needs updating if YT changes the function name.

```go
// sig.Cache holds parsed ops + compiled goja program per base.js hash.
type Cache struct { mu sync.RWMutex; entries map[string]*entry }

func (c *Cache) DecodeN(ctx context.Context, rawURL string) (string, error)
func (c *Cache) ApplySig(ctx context.Context, cipher string) (string, error)
```

Both methods fetch and parse `base.js` lazily on first call, then cache by the
player-JS-ID embedded in the URL. A new player-JS-ID in any response triggers a
background re-fetch.

### 403 sustained-burst handling

The `Cache` tracks consecutive 403s per base.js hash. After 3 failures in a 60s
window it evicts the entry and re-fetches `base.js` on the next call. If 403s
persist after re-fetch, it returns `ErrSigInvalidated` so the caller can alert.

---

## 5. Parsers

### Contract

```go
// Every Parse* function: takes raw json.RawMessage, returns typed struct, never errors.
func ParsePlayerResponse(raw json.RawMessage) models.PlayerResponse
func ParseNextPage(raw json.RawMessage) models.NextPage
func ParseHomePage(raw json.RawMessage) models.HomePage
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

This makes the zero-on-missing guarantee automatic with no nil checks scattered
through parser code.

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

Parsers return a `Cursor` inside typed response structs. The client base64-decodes
a non-zero `Cursor` into the POST body's `continuation` field.

---

## 7. Cookie store

```go
// cookies/store.go
// Key is [32]byte from SUNFLOWER_COOKIE_KEY (hex-decoded).
// Nonce is random 24 bytes prepended to ciphertext.
func Store(ctx context.Context, db *pgxpool.Pool, key [32]byte, provider string, raw []byte) error
func Load(ctx context.Context, db *pgxpool.Pool, key [32]byte, provider string) ([]byte, error)
```

Uses `golang.org/x/crypto/nacl/secretbox` — same `crypto_secretbox_xsalsa20poly1305`
primitive as libsodium, pure Go, no CGo.

`refresh_job.go` runs hourly: calls `Client.Next` against a known-stable video
ID with the stored cookies, writes `status = "ok"|"degraded"` to a
`cookie_health` table row. `GET /api/v1/cookies/youtube/status` reads that row.
A new migration (`0006_cookie_health.sql`) adds this table — single row keyed by
`provider`, upserted on each probe run.

---

## 8. Error handling summary

| Tier | Behaviour |
|---|---|
| Network / non-2xx | Wrapped error returned to caller; client retries once on 5xx |
| 403 sustained burst | Evict sig cache entry, re-fetch base.js; `ErrSigInvalidated` if persists |
| Parser missing field | Zero value returned; unknown renderer kind logged at debug, skipped |
| Cookie decrypt failure | Error returned; health probe marks status "degraded" |

---

## 9. Testing

| Layer | Method |
|---|---|
| `sig/transform.go` | Unit tests: frozen `(ops, input) → expected` pairs, no network |
| `sig/nsig.go` | Unit tests: frozen base.js snippet + known `n → decoded_n` pairs |
| `parser/*.go` | Table-driven tests against `testdata/` fixtures; normal + missing-field cases per surface |
| `cookies/store.go` | Unit test: encrypt → decrypt round-trip; tampered ciphertext errors |
| `probe innertube next` | Manual end-to-end only (requires live YT); not in CI |

No test containers needed for M3. Cookie DB interaction is covered by M1's
existing integration test harness when the cookie endpoint is wired in.

---

## 10. Out of scope for M3

- Personalized home/explore surface assembly (M5)
- Stream proxy fallback (M4)
- `internal/recs` integration
