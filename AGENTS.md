# Agent guidelines

This file provides guidance to agents when working with code in this repository.

## What this repository is

**Sunflower** is a self-hosted music system: a Go server (`sunflowerd`) + a cross-platform Flutter client. The system is currently in the **planning phase** — no implementation code exists yet. All content is design documents and milestone plans.

- `plans/README.md` — overview, locked decisions, milestone index
- `plans/architecture.md` — static reference: component map, wire protocol, Postgres schema, server and client internals
- `plans/milestones/m0–m8.md` — ordered build phases with acceptance criteria
- `plans/risks.md` — top risks and v1 out-of-scope list
- `docs/` — research notes on Metrolist (the Android reference implementation)

## Planned structure

### Server (`server/`)
Go + chi + Postgres, `sqlc` for the query layer. Key packages:

| Package | Purpose |
|---|---|
| `cmd/sunflowerd` | Entrypoint, env/flag parsing, wire-up |
| `cmd/probe` | Dev CLI for poking endpoints / InnerTube |
| `internal/innertube` | Native Go InnerTube: sig decryption, payloads, parser, continuation |
| `internal/recs` | Section builders, candidate pipeline, ranking, daily cache |
| `internal/library` | File scan (fsnotify), tag extraction (dhowden/tag), cover art (disintegration/imaging) |
| `internal/streams` | URL resolution: local path → YT direct → proxy |
| `internal/streamproxy` | Range-supporting reverse proxy with HMAC-signed tokens |
| `internal/sync` | Idempotency-key dedupe, write-replay, last-write-wins conflict resolution |
| `internal/cookies` | YT cookie storage (libsodium secretbox), refresh job |
| `internal/db` | sqlc-generated query layer |
| `internal/api` | chi router, handlers, middleware |
| `internal/ws` | WebSocket hub for now-playing |
| `internal/jobs` | Background workers |

### Client (`client/lib/`)
Flutter, feature-first layout. Core packages: `core/{api,auth,db,sync,player,downloads,media_session,network}`. Feature packages: `features/{home,player_ui,library,search,downloads_ui,settings}`.

## Key design decisions (locked)

- **Auth**: single-user, long-lived opaque device tokens (no refresh). Tokens stored as `argon2id` hashes.
- **media_id format**: `"<source>:<id>"` — e.g. `"yt:dQw4w9WgXcQ"`, `"local:01HZ…"` (local uses `sha1(path)[:16]`).
- **Cookie encryption**: libsodium `crypto_secretbox` in the app layer (key from `SUNFLOWER_COOKIE_KEY`); never touches Postgres.
- **InnerTube**: native Go reimplementation, no Python sidecar. Parsers must be optional-field tolerant (zero value on missing, never error).
- **Stream delivery**: client-direct-to-origin by default; server proxy only as 403/CORS fallback with HMAC-signed 5-min tokens.
- **Sync**: server is source of truth; client buffers mutations offline in `PendingMutations` (Drift) and replays by `client_clock` ASC; last-write-wins by `occurred_at`.
- **All mutating API calls** require an `Idempotency-Key` header (UUIDv7).
- **Write-replay buffer cap**: 10 000 entries; overflow evicts oldest non-confirmed, preserving likes over impression events.

## Milestone order

Work milestones in order — each depends on the prior being stable:

| # | Demo target |
|---|---|
| M0 | `sunflowerd` boots, `/healthz` OK, migrations applied |
| M1 | Device registers; library scan populates songs/albums/artists |
| M2 | Flutter app plays a local-library track end-to-end (Android first) |
| M3 | `probe innertube next --video-id=…` returns a playable URL |
| M4 | Client plays YT tracks with `/next` lookahead and 403 re-resolve |
| M5 | Home feed populated; cold-start renders cached sections |
| M6 | Playlist downloaded; airplane-mode playback works |
| M7 | Offline likes/edits drain to server in clock order, idempotent |
| M8 | Live now-playing push; optional crossfade |

## `internal/innertube` structure

- `sig/` — fetch `base.js`, extract sig function, cache by hash; invalidate on sustained 403 burst.
- `payloads/` — POST body builders for `/youtubei/v1/{player,next,browse,search}` with `ANDROID_MUSIC` context.
- `parser/` — one file per surface (`home_page.go`, `next_page.go`, …); optional-field-tolerant.
- `parser/testdata/` — committed fixture corpus to catch renderer drift.
- `continuation/` — opaque tokens preserved as `[]byte`, posted back verbatim.

## `internal/recs` design

Fan-out via `errgroup`, capped at 5 concurrent InnerTube calls per home build, 8s per-call timeout. Failed sections drop silently.

Ranking formula: `0.35·sourceAffinity + 0.20·seedStrength + 0.15·recency + 0.15·novelty + 0.10·remoteConfidence + 0.05·diversityBoost`

Cache TTLs: home/explore 30 min · similar-to 6 h · daily-discover until next midnight in user TZ · community playlists 24 h · radio/automix uncached.

## Flutter player layer

`core/player/sunflower_audio_handler.dart` extends `BaseAudioHandler`, holds `AudioPlayer` + `ConcatenatingAudioSource` from the lookahead buffer.

- Buffer < 4 items on transition → fetch next page in background.
- HTTP 403 or expiry → call `/api/v1/streams/resolve`, swap AudioSource URL, resume from position.
- Server unreachable → play through lookahead; on exhaustion fall back to `LocalRadio.fromRecentLikes()` (Drift query).
- Platform capability matrix lives in `core/player/capabilities.dart`.

## `/next` endpoint shape (the critical contract)

```
GET /api/v1/next?queue_id=&current_media_id=&audio_quality=
→ {
    current:   { media_id, source, stream_url, stream_expires_at, itag, mime_type,
                 content_length, loudness_db, playback_tracking_url, metadata },
    lookahead: [ …up to 8, same shape… ],
    continuation: "qc_…",
    automix:   [ …3 suggestion items, not committed… ],
    queue_version: N
  }
```
