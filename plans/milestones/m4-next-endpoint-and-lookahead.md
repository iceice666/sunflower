# M4 — Mixed Catalog + `/api/v1/next` + Lookahead

## Demo target

- Start playback of a YouTube song in the Flutter app.
- Server returns `current` + 5 lookahead items.
- Let the URL expire mid-stream → client auto-resolves via
  `POST /api/v1/streams/resolve` → playback resumes without user-visible glitch.
- Disconnect server mid-playback → client plays through the 5 buffered tracks
  → after buffer is exhausted, falls back to local-radio queue from likes /
  recent plays.

## Scope

- Queue session storage and lifecycle (`internal/queue`).
- `POST /api/v1/queue/start` — accepts a seed (song / album / playlist /
  artist_radio / shuffle_liked) and builds an initial queue.
- `GET /api/v1/next` — returns current + lookahead + continuation.
- `POST /api/v1/streams/resolve` — re-resolve on 403 / expiry.
- Stream URL resolver (`internal/streams`) — local-path vs YT-direct vs
  proxy decision.
- Stream proxy fallback (`internal/streamproxy`) with Range support.
- Flutter client: lookahead cache, `PlayerException` 403 handler, local-radio
  fallback.

## Files to create

```
server/internal/queue/
  session.go                  # queue_session lifecycle: create, mutate, materialize
  radio.go                    # YT-radio expansion (mirrors YouTubeQueue.radio)
  automix.go                  # automix shelf builder
server/internal/streams/
  resolver.go                 # source dispatch: local | youtube | proxy
  expiry.go                   # extract expires_at from googlevideo URL
server/internal/streamproxy/
  proxy.go                    # Range-aware reverse proxy
  token.go                    # HMAC short-lived token
server/internal/api/
  handlers_queue.go           # POST /queue/start, GET /queue/{id}, POST /queue/{id}/mutate
  handlers_next.go            # GET /next
  handlers_streams.go         # POST /streams/resolve

client/lib/core/db/
  database.dart               # Drift: LookaheadCache, RecentPlays
client/lib/core/player/
  sunflower_audio_handler.dart   # extends BaseAudioHandler, owns ConcatenatingAudioSource
  lookahead_loader.dart       # fetches /next, fills the buffer ≥4
  expiry_guard.dart           # listens for 403 or near-expiry → /streams/resolve
  local_radio.dart            # fallback queue from likes + recent plays
client/lib/features/
  player_ui/queue_panel.dart  # show upcoming items
```

## Acceptance criteria

- Starting a queue from a YouTube song seed returns `queue_id` with ≥10 items
  pre-materialized.
- `GET /api/v1/next` always includes `current` plus 5–8 lookahead items.
- `stream_expires_at` is set correctly for YouTube sources and null for local.
- Expiring the URL artificially (force expires_at to past) → next playback
  attempt triggers `/streams/resolve` exactly once → playback resumes from the
  same position.
- Killing the server during playback → 5 buffered tracks all play to
  completion → after buffer empty, local radio kicks in with at least 10
  fallback tracks if any likes/recent plays exist locally.
- Range requests work on the stream proxy (test with `curl -r 1000-2000`).

## Dependencies on prior milestones

- M3 InnerTube client.
- M2 Flutter player baseline.

## Verification

- Unit test: `streams.Resolver.Resolve` decision table for each source type
  with edge cases (expired-but-cached, local-file-missing, YT-blocked).
- Unit test: lookahead buffer maintains ≥4 items under simulated playback.
- Integration test: end-to-end `/queue/start` → `/next` → `/streams/resolve` with
  a fake InnerTube returning a soon-to-expire URL.
- Manual test: airplane mode mid-song; verify local-radio fallback engages
  exactly when buffer empties, not before.

## Out of M4 scope

- Recommendations driving queue contents (M5).
- WebSocket now-playing push (M8).
- Queue persistence to Drift across app restarts — for M4, lookahead is
  in-memory + Drift cache for cold-start; full restore comes in M7.

## Implementation status

**Server half: done.** Implemented and verified (`go build`/`vet`/`test` incl.
testcontainers integration, `gofmt`, `sqlc` all green):

- `internal/queue` — session lifecycle + materialization (`session.go`), YT
  radio expansion via `/next` continuations (`radio.go`), shuffle-liked automix
  (`automix.go`).
- `internal/streams` — source-dispatch resolver local|youtube|proxy
  (`resolver.go`) and googlevideo expiry extraction (`expiry.go`).
- `internal/streamproxy` — HMAC-SHA256 short-lived tokens (`token.go`) and a
  Range-aware proxy with host allowlist + per-redirect SSRF re-validation
  (`proxy.go`).
- Handlers: `POST /queue/start`, `GET /queue/{id}`, `GET /next`,
  `POST /streams/resolve`, `GET /streams/proxy` (token-authorized, outside the
  device-auth group).
- Seed kinds implemented: `song` (YT radio) and `shuffle_liked`. `album` /
  `playlist` / `artist_radio` deferred (the album/artist browse parsers do not
  yet return track lists).

**Client half: pending follow-up** (no Flutter/Dart toolchain in this
environment). Outstanding: lookahead cache (Drift), `PlayerException` 403
handler → `/streams/resolve`, local-radio fallback, and the queue panel UI.
