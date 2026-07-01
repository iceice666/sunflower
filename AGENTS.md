# Agent guidelines

This file provides guidance to agents when working with code in this repository.

## What this repository is

**Sunflower** is a self-hosted music system: a Rust server/core (`sunflowerd-rs`) + a cross-platform Flutter client.

**Current status (2026-07-01):** M0–M10 are implemented on the Rust stack. The `rust/` workspace is the canonical backend/core implementation (auth, secure enrollment, browser admin dashboard, library, native InnerTube, queue/streams, recs, downloads registry, sync/idempotency, now-playing WebSocket, local recommendation core, Postgres and SQLite storage, Flutter Rust Bridge). The old Go `server/` implementation has been removed; retained SQL migrations, admin static assets, and InnerTube fixtures now live under Rust crates. The `client/` tree is a complete Flutter project (pairing-first onboarding, player + queue/lookahead, home feed, offline downloads, write-replay buffer, now-playing socket, local recommendation fallback). Rust is verified with `cargo test --workspace --locked`, `cargo clippy --workspace --locked --all-targets -- -D warnings`, and `cargo fmt --all -- --check`; Flutter is verified with `flutter analyze` and targeted tests/goldens.

- `plans/README.md` — overview, locked decisions, milestone index
- `plans/architecture.md` — static reference: component map, wire protocol, Postgres schema, server and client internals
- `plans/milestones/m0–m10.md` — ordered build phases with acceptance criteria
- `plans/risks.md` — top risks and v1 out-of-scope list
- `docs/` — research notes on Metrolist (the Android reference implementation)

## Planned structure

### Rust workspace (`rust/`)
Rust 2024 + axum + Postgres/SQLite. Key crates:

| Crate | Purpose |
|---|---|
| `sunflower-server` | Runnable `sunflowerd-rs`: env/flag parsing, axum router, handlers, admin UI, jobs, native InnerTube, stream proxy, WebSocket |
| `sunflower-core` | Shared domain/wire types, queue/lookahead logic, local recommendation ranking |
| `sunflower-storage-postgres` | Source-of-truth Postgres storage and embedded schema migrations |
| `sunflower-storage-sqlite` | Device-local SQLite storage for local mode/stat snapshots |
| `sunflower-bridge` | Flutter Rust Bridge API used by the Flutter client |

### Client (`client/lib/`)
Flutter, feature-first layout. Core packages: `core/{api,auth,db,sync,player,downloads,media_session,network}`. Feature packages: `features/{home,player_ui,library,search,downloads_ui,settings}`.

## Key design decisions (locked)

- **Auth**: single-user, long-lived opaque device tokens (no refresh). Tokens stored as `argon2id` hashes.
- **media_id format**: `"<source>:<id>"` — e.g. `"yt:dQw4w9WgXcQ"`, `"local:01HZ…"` (local uses `sha1(path)[:16]`).
- **Cookie encryption**: libsodium `crypto_secretbox` in the app layer (key from `SUNFLOWER_COOKIE_KEY`); never touches Postgres.
- **InnerTube**: native Rust implementation, no Python sidecar. Parsers must be optional-field tolerant (zero value on missing, never error).
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
| M3 | Rust InnerTube player/next/search paths return playable URLs and parsed sections |
| M4 | Client plays YT tracks with `/next` lookahead and 403 re-resolve |
| M5 | Home feed populated; cold-start renders cached sections |
| M6 | Playlist downloaded; airplane-mode playback works |
| M7 | Offline likes/edits drain to server in clock order, idempotent |
| M8 | Live now-playing push; optional crossfade |

## Rust InnerTube structure

- `rust/crates/sunflower-server/src/innertube.rs` — HTTP client, payloads, parsers, continuation/radio expansion helpers.
- `rust/crates/sunflower-server/testdata/innertube/` — committed fixture corpus to catch renderer drift.

## Recommendation design

Remote fan-out is capped and timeout-bound in `sunflower-server`; failed sections drop silently. Device-local fallback ranking lives in `sunflower-core` and `sunflower-storage-sqlite`.

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
