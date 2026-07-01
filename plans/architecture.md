# Architecture Reference

Static reference: component map, wire protocol shapes, Postgres schema,
server internals, client internals. Each milestone in `milestones/` cites
sections of this file rather than restating them.

## Component map

### Rust workspace — `rust/`

| Crate | Purpose |
|---|---|
| `sunflower-core` | Shared domain and wire contract types, queue/lookahead logic, local recommendation ranking, repository traits |
| `sunflower-server` | Runnable Rust `sunflowerd-rs`: axum router, auth/admin/API handlers, native InnerTube, queue/streams, recs, jobs, now-playing WebSocket |
| `sunflower-storage-postgres` | Online/source-of-truth storage; applies embedded schema migrations, then Rust-local extension tables |
| `sunflower-storage-sqlite` | Device-local recommendation storage for mobile/desktop local mode |
| `sunflower-bridge` | Flutter Rust Bridge API for local songs, stat snapshots, ranking, recommendation snapshots, and feedback replay state |

The old Go `server/` tree has been removed. Assets that still matter to the
Rust implementation live with the Rust crates:

- Postgres schema: `rust/crates/sunflower-storage-postgres/migrations/`
- Admin CSS/JS: `rust/crates/sunflower-server/assets/admin/`
- InnerTube parser fixtures: `rust/crates/sunflower-server/testdata/innertube/`

Rust contract tests preserve the established public wire shapes, status codes,
route behavior, parsers, stream proxy policy, idempotency behavior, and selected
storage semantics.

### Flutter client — `client/lib/` (feature-first)

```
core/
  api/                # generated DTOs + dio client + retry-on-403
  auth/               # token store, device-register flow
  bridge/             # generated Flutter Rust Bridge bindings
  db/                 # Drift schema + DAOs
  sync/               # write-replay buffer, idempotency keys (UUIDv7)
  player/             # just_audio + audio_service handler
  downloads/          # background isolate download manager
  media_session/      # via audio_service
  network/            # dio interceptors
  recommendations/    # FRB-backed local recommender + offline Home fallback
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

All application API JSON uses `Authorization: Bearer <device_token>`.
Mutating device endpoints require `Idempotency-Key` header (UUIDv7). Device
registration uses the same idempotency contract even though it is the endpoint
that issues the first bearer token. M9 adds a separate admin browser session
carried by an HttpOnly cookie; admin mutating forms and JSON calls require CSRF
protection.

### Bootstrap / setup status (M9)
```
GET /api/v1/setup/status
→ {
    configured: true|false,
    pairing_required: true,
    server_version,
    server_capabilities: ["auth.pairing.v1", "admin.dashboard.v1", ...]
  }
```

This endpoint is intentionally public and reveals no secrets. Clients use it
to distinguish "server not reachable", "server needs owner setup", and "pairing
required".

### Owner setup (M9)
```
POST /api/v1/setup/owner
{ setup_token, display_name, password }
→ { ok: true }
```

Allowed only while no owner password exists. `setup_token` comes from
`SUNFLOWER_SETUP_TOKEN` or an ephemeral first-run token printed to the server
console. The endpoint rate-limits by remote address and is disabled forever
after owner setup succeeds.

### Admin auth (M9)
```
POST /api/v1/admin/auth/login
{ password }
→ Set-Cookie: sf_admin=...; HttpOnly; SameSite=Lax; Secure when HTTPS
→ { csrf_token, expires_at }

POST /api/v1/admin/auth/logout
→ { ok: true }

GET /api/v1/admin/me
→ { user_id, display_name, session_expires_at }
```

Admin tokens are random high-entropy session secrets stored as hashes in
Postgres. Browser sessions are not device tokens and cannot be used for player
API calls.

### Pairing and device registration (M9)
```
POST /api/v1/admin/pairing-codes
{ label, ttl_seconds }
→ {
    pairing_code,
    pairing_url,
    expires_at
  }

POST /api/v1/auth/register-device
Idempotency-Key: <uuidv7>
{ device_name, platform, client_version, pairing_code }
→ {
    device_id,
    token,
    server_capabilities: ["auth.pairing.v1", "recs.v1", "stream.proxy", "ws.now_playing"]
  }
```

Pairing codes are single-use, expire after 10 minutes by default, and are shown
only once. The server stores only an HMAC/argon2id-derived verifier, never the
raw code. Registration without a valid pairing code returns
`403 {"error":"pairing_required"}` or `401 {"error":"invalid_pairing_code"}`.
Replaying the same registration `Idempotency-Key` on the same route within 24
hours returns the original token response without consuming or re-checking the
single-use pairing code.

### Device API auth
```
Authorization: Bearer sf_dev_...
```

Device tokens are opaque 32 random bytes, stored as hashes, and have no refresh
token. M9 adds `revoked_at`; revoked devices get `401 {"error":"device_revoked"}`
on HTTP and WebSocket requests. A revoked client clears local credentials and
returns to the pairing screen.

### Admin dashboard (M10)
```
GET /admin/                 # HTML overview, redirects to /admin/login if needed
GET /admin/login            # HTML login form
GET /api/v1/admin/status    # JSON dashboard payload
GET /api/v1/admin/devices   # JSON device list
POST /api/v1/admin/devices/{id}/revoke
POST /api/v1/admin/library/scan
POST /api/v1/admin/cookies/youtube
POST /api/v1/admin/now-playing/command
```

`/api/v1/admin` from M8 remains a compatibility alias for the JSON status
payload until callers migrate to `/api/v1/admin/status`. The browser UI is
server-rendered HTML, not a separate SPA.

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
              total_played_ms, duration_ms, reason } ] }
→ { results: [{event_id, accepted, reason?}] }
```
Each event carries its own UUIDv7 event id; the request also carries the
batch-level `Idempotency-Key`. The batch key replays the exact HTTP response for
same-request retries; the per-event id prevents double-scrobbling when the client
re-batches the same event under a new HTTP key. The server processes a batch in
request order and returns `results` in that same order; client replay buffers are
responsible for draining buffered mutations by `client_clock`. "Conflict" on
mutations = duplicate key, not state collision.

### Local recommendation mode

When the remote Home/recommendation surface is unreachable, the Flutter client
builds a stale `HomeFeed` from device-local state:

- Drift `RecentPlays` and downloaded-track rows provide playable candidates.
- `sunflower-storage-sqlite` stores local songs, recommendation events, and the
  latest remote snapshot.
- `sunflower-core::LocalRecommendationEngine` ranks candidates from a
  `LocalStatsSnapshot`.
- Playback-start events stay local-only. Remote-contract feedback
  (`playCompleted`, `skipped`, `liked`/`disliked`, `impression`) is converted
  back to the legacy `/events`, `/likes`, and `/impressions` shapes and sent by
  the recommendation feedback client with the same UUIDv7 as the
  `Idempotency-Key`. The local Rust SQLite event log remains the durable retry
  source until those sends succeed.

Home reads use `recommendationApiProvider`, whose base URL is the optional
standalone recommendation server when configured and the main server otherwise.
The optional URL can be stored in secure storage or supplied at build/run time
with `--dart-define=SUNFLOWER_RECOMMENDATION_URL=...`.
Library, queue, playlist, download, and source-of-truth sync calls continue to
use the main `sunflowerApiProvider` / write-replay buffer.

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
  -- M9 adds revoked_at, revoked_reason, token_label

admin_sessions (id, user_id, token_hash UNIQUE, csrf_secret_hash,
                expires_at, last_seen_at, revoked_at, created_at)
pairing_codes  (id, user_id, code_hash, label, expires_at, used_at,
                used_by_device_id, created_by_session_id, created_at)
audit_events   (id, user_id, actor_type, actor_id, event, target_type,
                target_id, metadata jsonb, created_at)

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
                 response_status, response_body, response_content_type,
                 created_at, expires_at)
rec_cache (cache_key PK, user_id, payload jsonb, generated_at, expires_at)
rust_ingested_events (user_id, event_id PK, device_id, created_at)

-- Rust rewrite extension tables
rust_songs (media_id PK, source_type, title, available, payload jsonb, updated_at)
rust_recommendation_events (event_id PK, media_id, client_clock, occurred_at,
                            kind, payload jsonb, synced_at)
rust_recommendation_snapshots (snapshot_id PK, model_version, generated_at,
                               expires_at, payload jsonb)
rust_like_tombstones (user_id, song_media_id PK, unliked_at, idempotency_key)
```

Cookie encryption: libsodium `crypto_secretbox` with a 32-byte key from
`SUNFLOWER_COOKIE_KEY`. Not `pgcrypto` — encryption stays in the app layer so
the key never reaches Postgres.

## SQLite local recommendation schema

The client-local Rust store mirrors only the data needed for offline ranking
and feedback replay:

```sql
songs (media_id PK, source_type, title, available, payload_json, updated_at)
recommendation_events (event_id PK, media_id, client_clock, occurred_at, kind,
                       payload_json, synced_at)
recommendation_snapshots (snapshot_id PK, generated_at, expires_at,
                          model_version, payload_json)
```

Local-library songs are considered locally available when `source_type=local`
and `available=true`, even if the mobile bridge did not persist a concrete file
path. Downloaded remote tracks are considered both downloaded and locally
available when they have a local path.

## Server internals — notable parts

### `sunflower-server::innertube`
Mirrors Metrolist's Kotlin `innertube` module in Rust:
- `sig/` — fetch `base.js`, regex-extract sig function, parse its op list
  (reverse/splice/swap), apply in pure Rust. Cache by base.js hash; invalidate
  on sustained 403.
- `payloads/` — POST body builders for `/youtubei/v1/{player,next,browse,search}`
  with `ANDROID_MUSIC` client context.
- Parser helpers normalize renderer surfaces for home, next, related, artist,
  album, playlist, and search. **Optional-field tolerant**: missing branches
  return zero values, never errors.
- Continuations are opaque strings/bytes and are posted back verbatim.

Cookie middleware on the HTTP client reads encrypted cookie state and attaches
`Cookie:` headers; it preserves the legacy provider formats.

### Remote recommendation engine
One function per Metrolist surface: `BuildHome`, `QuickPicks`,
`DailyDiscover`, `SimilarToArtist`, `SimilarToSong`, `SimilarToAlbum`,
`CommunityPlaylists`, `Radio`.

- **Fan-out:** async Rust tasks per home build, per-seed sub-fanout capped at 5
  concurrent InnerTube calls, 8s per-call timeout. Failed similar-to sections
  are dropped, not propagated.
- **Filter pipeline:** composable candidate predicates — `notExplicit`,
  `notVideo`, `notShorts`, `notBlocked`, `notRecentImpression(<24h)`,
  `notDuplicateInSection`.
- **Ranking** (per docs):
  `0.35·sourceAffinity + 0.20·seedStrength + 0.15·recency + 0.15·novelty + 0.10·remoteConfidence + 0.05·diversityBoost`
- **Cache TTLs:** home/explore 30 min, similar-to 6 h, daily-discover until
  next midnight in user TZ, community playlists 24 h, radio/automix not cached.
- Cache key includes user, source, seed, locale, region, filters hash.

### Library scanner (`sunflower-server::jobs`)
- Tag extraction: Rust ID3 parsing plus filename fallback for non-MP3 audio.
- `media_id = "local:" + sha1(path)[:16]` for stability across rescans.
- Cover art: resize to 256/512/1024 with the Rust `image` crate, store under
  `<data>/art/<media_id>/{256,512,1024}.jpg`.

### Stream proxy (`sunflower-server::stream_proxy`)
Fallback path only — reqwest-backed reverse proxy with `Range` forwarding,
HMAC-signed short-lived tokens to prevent open-proxy abuse, no disk buffering.

### Sync/idempotency (`sunflower-server` + `sunflower-storage-postgres`)
- Middleware reads `Idempotency-Key` on all mutations.
- Cache hit within 24 h → replay stored response. Stale → 409 with conflict.
- Conflict resolution: last-write-wins by client `occurred_at`. Returns
  `{accepted, server_state}` so client can reconcile its local view.

### Auth/enrollment model (M9)
- First-run setup is owner-only. Once an owner password hash exists, setup
  endpoints reject all future calls.
- Admin passwords are stored as PHC-format argon2id hashes with per-password
  salts. Device tokens remain opaque high-entropy random strings and are never
  displayed after creation.
- Pairing codes are short-lived, single-use, and rate-limited. Human entry uses
  Crockford Base32 without ambiguous characters; QR pairing uses the same raw
  code in a `sunflower://pair?...` or HTTPS URL.
- Device auth middleware rejects missing, invalid, or revoked tokens before
  handlers run. The WebSocket token query-param fallback remains, but it has
  the same revocation and last-seen behavior as `Authorization`.
- Every security-sensitive event writes `audit_events`: owner setup, admin
  login success/failure, pairing code created/used/expired, device revoked,
  YouTube cookies updated, and library scan started from admin.

### Admin dashboard model (M10)
- Server-rendered HTML with embedded Rust string/static assets. No React/Vue
  build step for v1 dashboard operations.
- All dashboard pages require the admin session cookie. HTML forms include a
  per-session CSRF token; JSON admin mutations require `X-CSRF-Token`.
- Pages: overview, devices, library scans, YouTube cookies, now-playing, audit
  log, and basic security settings.
- The dashboard never displays device bearer tokens or raw YouTube cookies.
  Pairing codes are displayed once at creation time.

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
