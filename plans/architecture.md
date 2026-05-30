# Architecture Reference

Static reference: component map, wire protocol shapes, Postgres schema,
server internals, client internals. Each milestone in `milestones/` cites
sections of this file rather than restating them.

## Component map

### Go server — `server/`

| Package | Metrolist analogue | Purpose |
|---|---|---|
| `cmd/sunflowerd` | — | Main entrypoint, env/flag parsing, wire-up |
| `cmd/probe` | — | Dev CLI for poking endpoints / InnerTube |
| `internal/auth` | — | Device registration, token middleware |
| `internal/library` | local Room song table | File scan (fsnotify + walker), tag extraction via `dhowden/tag`, cover art thumbs via `disintegration/imaging` |
| `internal/innertube` | `innertube` module | Native Go InnerTube: sig decryption, player-response parsing, home/next/related/artist/album/playlist, continuation, renderer normalizers |
| `internal/recs` | `HomeViewModel` + `RecommendationRepository` | Section builders, candidate pipeline, ranking, daily cache, filter pipeline |
| `internal/queue` | `Queue` + `YouTubeQueue` | Server-side queue session, radio expansion, automix shelf |
| `internal/streams` | `YTPlayerUtils` | URL resolution decision tree (local path vs YT direct vs proxy) |
| `internal/streamproxy` | `ResolvingDataSource` fallback path | Range-supporting reverse proxy with short-lived HMAC tokens |
| `internal/sync` | — | Idempotency-key dedupe, write-replay handling, conflict resolution |
| `internal/events` | `PlaybackStatsListener` | Play event ingestion, impression logging, scrobble window |
| `internal/cookies` | — | YT cookie storage (libsodium secretbox), refresh job |
| `internal/db` | Room | `sqlc`-generated query layer |
| `internal/api` | — | chi router, handlers, JSON shapes, middleware |
| `internal/ws` | — | WebSocket hub for now-playing |
| `internal/jobs` | — | Background workers: cookie health probe, library scan, recs warmup |

### Flutter client — `client/lib/` (feature-first)

```
core/
  api/                # generated DTOs + dio client + retry-on-403
  auth/               # token store, device-register flow
  db/                 # Drift schema + DAOs
  sync/               # write-replay buffer, idempotency keys (UUIDv7)
  player/             # just_audio + audio_service handler
  downloads/          # background isolate download manager
  media_session/      # via audio_service
  network/            # dio interceptors
features/
  home/               # sections feed, cold-start cache
  player_ui/          # now-playing, queue panel
  library/            # songs/albums/artists/playlists screens
  search/
  downloads_ui/
  settings/
```

Feature-first because the client has ~6 distinct surfaces and most code is
UI-adjacent; layer-first would scatter related changes across 4 folders per
surface.

## Wire protocol — load-bearing endpoints

All JSON. `Authorization: Bearer <token>`. Mutating endpoints require
`Idempotency-Key` header (UUIDv7).

### Device registration
```
POST /api/v1/auth/register-device
→ { device_id, token, server_capabilities: ["recs.v1","stream.proxy","ws.now_playing"] }
```
Token is opaque 32 random bytes, stored as `argon2id` hash. No refresh.

### Next-track decision (the novel piece)
```
GET /api/v1/next?queue_id=&current_media_id=&audio_quality=
→ {
    current:  { media_id, source: "local"|"youtube"|"proxy",
                stream_url, stream_expires_at, itag, mime_type,
                content_length, loudness_db, playback_tracking_url, metadata },
    lookahead: [ ... up to 8 entries, same shape ... ],
    continuation: "qc_…",
    automix: [ ... 3 suggestion items, not committed to queue ... ],
    queue_version: N
  }
```
- `source=local` → `stream_url` is server (or LAN-direct) URL, `stream_expires_at` null.
- `source=youtube`/`proxy` → expiring googlevideo URL, `stream_expires_at` ~5h out.
- Lookahead is the **offline prefetch buffer** — client plays through it if server is unreachable.

### 403 / expiry re-resolve
```
POST /api/v1/streams/resolve  {media_id, audio_quality, reason}
→ { media_id, source, stream_url, stream_expires_at }
410 → { error: "media_unavailable", alternative_media_id? }
```

### Play events (batched, idempotent)
```
POST /api/v1/events
{ events: [ { event_id (UUIDv7), kind, media_id, queue_id, occurred_at,
              elapsed_ms, total_played_ms, reason } ] }
→ { results: [{event_id, accepted, conflicted_with?, reason?}] }
```
Each event carries its own idempotency key; the request also carries a batch
key. "Conflict" on mutations = duplicate key, not state collision.

### Now-playing push — WebSocket
```
WS /api/v1/ws/now-playing   (subprotocol sunflower.now-playing.v1)
client → server  { type: "tick"|"transition"|"state",
                   queue_id, media_id, position_ms, duration_ms,
                   is_playing, shuffle, repeat }
server → client  { type: "command", command: "pause"|"skip_next"|… }
```
WebSocket chosen over POST/SSE because client emits ~1Hz position ticks; one
persistent socket beats one HTTP request per tick.

### Summarized rest

- Library CRUD: `GET/POST/PATCH/DELETE /api/v1/library/{songs|albums|artists|playlists}`.
- `POST /api/v1/library/scan {roots}` → `{job_id}`; progress via `GET /api/v1/jobs/{id}`.
- `POST /api/v1/likes {media_id, liked}` — last-write-wins by `occurred_at`.
- `POST /api/v1/queue/start {seed_kind, seed_id, shuffle, preserve_existing}`.
- `POST /api/v1/cookies/youtube` — server encrypts immediately, never echoes back.

## Postgres schema (key tables)

```sql
users (id, display_name, created_at)
devices (id, user_id, name, platform, token_hash, last_seen_at, created_at)

-- media_id = "<source>:<external_id>", e.g. "yt:dQw4w9WgXcQ", "local:01HZ…"
songs   (media_id PK, source_type, title, duration_ms, album_id,
         primary_artist_id, explicit, video_only, available, loudness_db,
         last_resolved_at, raw_metadata jsonb)
albums  (media_id PK, …)
artists (media_id PK, …)
song_artists (song_media_id, artist_media_id, position)

playlists      (id, user_id, title, source_type, external_id, version)
playlist_items (playlist_id, position, song_media_id, added_at, added_by_device_id)

play_events (id PK, user_id, device_id, song_media_id, queue_id, kind,
             occurred_at, total_played_ms, reason)
  -- idx (user_id, song_media_id, occurred_at DESC) for most-played
  -- idx (user_id, occurred_at DESC) for recent + forgotten-favorites query

likes (user_id, song_media_id PK, liked_at, idempotency_key UNIQUE)
downloaded_tracks (device_id, song_media_id PK, local_path, bytes,
                   completed_at, last_verified_at)
recommendation_impressions (id, user_id, section_id, source, seed_id,
                            media_id, shown_at, clicked_at, position)
queue_sessions (id, user_id, device_id, seed_kind, seed_id, version, title,
                items jsonb)
queue_items    (queue_id, position PK, media_id, source_data jsonb)

encrypted_cookies (user_id, provider PK, ciphertext bytea, nonce bytea,
                   refreshed_at, expires_at_hint)

idempotency_log (key PK, user_id, device_id, route, response_hash,
                 created_at, expires_at)
rec_cache (cache_key PK, user_id, payload jsonb, generated_at, expires_at)
```

Cookie encryption: libsodium `crypto_secretbox` with a 32-byte key from
`SUNFLOWER_COOKIE_KEY`. Not `pgcrypto` — encryption stays in the app layer so
the key never reaches Postgres.

## Server internals — notable parts

### `internal/innertube`
Mirrors Metrolist's Kotlin `innertube` module but in Go:
- `sig/` — fetch `base.js`, regex-extract sig function, parse its op list
  (reverse/splice/swap), apply in pure Go. Cache by base.js hash; invalidate
  on sustained 403.
- `payloads/` — POST body builders for `/youtubei/v1/{player,next,browse,search}`
  with `ANDROID_MUSIC` client context.
- `parser/` — renderer normalizers, one file per surface
  (`home_page.go`, `next_page.go`, `related_page.go`, `artist_page.go`,
  `album_page.go`, `playlist_page.go`). **Optional-field tolerant**: missing
  branches return zero values, never errors.
- `continuation/` — opaque tokens, preserved as `[]byte`, posted back verbatim.

Cookie middleware on the HTTP client reads from `internal/cookies` and
attaches `Cookie:` headers; watches for `Set-Cookie` rotations.

### `internal/recs`
One function per Metrolist surface: `BuildHome`, `QuickPicks`,
`DailyDiscover`, `SimilarToArtist`, `SimilarToSong`, `SimilarToAlbum`,
`CommunityPlaylists`, `Radio`.

- **Fan-out:** `errgroup` per home build, per-seed sub-fanout capped at 5
  concurrent InnerTube calls, 8s per-call timeout. Failed similar-to sections
  are dropped, not propagated.
- **Filter pipeline:** composable `func(Candidate) bool` — `notExplicit`,
  `notVideo`, `notShorts`, `notBlocked`, `notRecentImpression(<24h)`,
  `notDuplicateInSection`.
- **Ranking** (per docs):
  `0.35·sourceAffinity + 0.20·seedStrength + 0.15·recency + 0.15·novelty + 0.10·remoteConfidence + 0.05·diversityBoost`
- **Cache TTLs:** home/explore 30 min, similar-to 6 h, daily-discover until
  next midnight in user TZ, community playlists 24 h, radio/automix not cached.
- Cache key includes user, source, seed, locale, region, filters hash.

### `internal/library`
- Tag extraction: pure-Go `dhowden/tag` (covers MP3/M4A/FLAC/OGG; avoids CGo).
- Watcher: `fsnotify` on roots, 2 s debounce.
- `media_id = "local:" + sha1(path)[:16]` for stability across rescans.
- Cover art: resize to 256/512/1024 with `disintegration/imaging`, store under
  `<data>/art/<media_id>/{256,512,1024}.jpg`.

### `internal/streamproxy`
Fallback path only — `httputil`-style reverse proxy with `Range` forwarding,
HMAC-signed short-lived tokens (5 min) to prevent open-proxy abuse, no disk
buffering.

### `internal/sync`
- Middleware reads `Idempotency-Key` on all mutations.
- Cache hit within 24 h → replay stored response. Stale → 409 with conflict.
- Conflict resolution: last-write-wins by client `occurred_at`. Returns
  `{accepted, server_state}` so client can reconcile its local view.

## Flutter client — notable parts

### Player layer (`core/player/sunflower_audio_handler.dart`)
Extends `BaseAudioHandler`. Holds the `AudioPlayer` and a
`ConcatenatingAudioSource` populated from the lookahead buffer.
- On `mediaItem` transition: pop the played item; if buffer < 4 items, fetch
  next page in background.
- On `PlayerException` HTTP 403 or expiry within X seconds: call
  `/api/v1/streams/resolve`, swap the AudioSource URL, resume from position.
- On `/api/v1/next` unreachable: play through the existing lookahead. When
  exhausted, fall back to `LocalRadio.fromRecentLikes()` — a Drift query
  returning N tracks from local likes + recent plays.

### Drift schema (client)
- `PendingMutations` — id (UUIDv7), route, idempotency_key UNIQUE, payload
  JSON, client_clock, attempts, status (pending|sent|confirmed|failed).
- `DownloadedTracks` — media_id PK, local_path, bytes, completed_at.
- `LookaheadCache` — (queue_id, position) PK, media_id, stream_url,
  stream_expires_at, source, metadata JSON.
- `HomeCache` — (section_id, position) PK, item JSON, cached_at — cold-start
  render before the server is reachable.
- `RecentPlays` — media_id, played_at — seed for fallback radio.

### Write-replay buffer
- Every mutation → write to `PendingMutations` (status=pending) → attempt
  network call → success: status=confirmed.
- Background isolate retries with exponential backoff (5 s → 30 s → 5 min →
  30 min → 2 h cap).
- **Buffer cap: 10 000 entries.** Overflow drops oldest non-confirmed; likes
  win over impressions on eviction priority.
- Replay order is by `client_clock` ASC; server applies last-write-wins by
  the same field.

### Background downloads
Custom Dart isolate using `dio`'s `download` API (chosen over
`flutter_downloader` because the latter is mobile-only). Queue persisted in
Drift; per-job pause/resume/cancel; chunks to
`getApplicationSupportDirectory()/downloads/<media_id>.<ext>`. SHA-256
verification for local-library files; YT downloads can't be checksummed
reliably. Web degrades to read-only catalog browsing in v1.

### OS media session
Configured via `audio_service` — lock-screen art, prev/next, like as a custom
action. No platform-specific glue beyond the config.
