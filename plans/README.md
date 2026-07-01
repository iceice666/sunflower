# Sunflower — Plan

A Metrolist-inspired self-hosted music system in two parts: a Flutter
cross-platform client and a Rust server/core. Rust is now the canonical
implementation: runnable server, shared domain core, Flutter Rust Bridge
surface, Postgres/SQLite storage, and local-mode recommendation engine. The
old Go `server/` implementation has been removed after the Rust rewrite reached
wire/behavior parity.

## Why this plan exists

The research in [`../docs/`](../docs/) describes Metrolist's Android-only
architecture — Media3/ExoPlayer client, on-device Room DB, on-device InnerTube
fan-out. The first implementation centralized the brain in Go `sunflowerd`.
The final Rust implementation keeps the same public wire contract while
splitting the system into:

- Rust server + Postgres storage for the online, source-of-truth API.
- Rust `sunflower-core` for queue/recommendation/domain logic shared by server
  and client-local mode.
- Rust SQLite storage behind Flutter Rust Bridge for device-local stat
  snapshots, local ranking, and offline feedback capture.
- Flutter UI/player/download/sync surfaces that can keep useful recommendation
  behavior when the remote recommendation surface is unreachable, then feed
  local feedback back through the recommendation feedback sink when
  connectivity returns.

## Locked decisions

| Area | Choice |
|---|---|
| Client | Flutter (iOS, Android, desktop, web), `just_audio` + `audio_service`, Drift |
| Catalog | Hybrid: self-hosted library files + YouTube Music fallback |
| Stream delivery | Client direct-to-origin by default; server proxy as fallback on 403/CORS |
| Server stack | Rust + axum + Postgres; SQLite for device-local mode |
| InnerTube | Native implementation in Rust server, matched against committed fixtures/contracts |
| YT personalization | Server holds user's YT cookies, encrypted at rest (libsodium) |
| Sync | Server is source of truth; client buffers writes offline and replays; now-playing pushed via WebSocket; rest is polling |
| Recommendations | Remote server builds canonical sections; client has Rust-core local ranking from SQLite stat snapshots for local mode |
| Offline | Explicit per-track / per-playlist downloads with local cache + DB |
| Auth | Single-user admin account; devices get long-lived opaque tokens only through admin-generated pairing codes (M9) |

## How to read this folder

- [`architecture.md`](architecture.md) — static reference: component map, wire
  protocol shapes, Postgres schema, server-internal designs, client-internal
  designs. Read this once; refer back during every milestone.
- [`milestones/`](milestones/) — archived build-phase notes for M0–M10. They
  preserve the original Go-era implementation plan and acceptance history, so
  some paths and commands inside those files intentionally reference the
  removed `server/` tree. Use this README and [`architecture.md`](architecture.md)
  as the current Rust implementation reference.
- [`risks.md`](risks.md) — top risks with mitigations, plus what is explicitly
  out of scope for v1.

## Milestone index

| # | Status | File | Demo target |
|---|---|---|---|
| M0 | complete | [`milestones/m0-server-bootstrap.md`](milestones/m0-server-bootstrap.md) | `sunflowerd` boots, `/healthz` returns OK, migrations applied |
| M1 | complete | [`milestones/m1-auth-and-library-ingestion.md`](milestones/m1-auth-and-library-ingestion.md) | Device registers, library scan populates songs/albums/artists |
| M2 | complete | [`milestones/m2-flutter-player-local-library.md`](milestones/m2-flutter-player-local-library.md) | Flutter app plays a local-library track end-to-end |
| M3 | complete | [`milestones/m3-innertube-client.md`](milestones/m3-innertube-client.md) | `probe innertube next --video-id=…` returns a fresh playable URL |
| M4 | complete | [`milestones/m4-next-endpoint-and-lookahead.md`](milestones/m4-next-endpoint-and-lookahead.md) | Client plays YT tracks with `/next` lookahead and 403 re-resolve |
| M5 | complete | [`milestones/m5-recommendation-pipeline.md`](milestones/m5-recommendation-pipeline.md) | Home feed populated; cold-start renders cached sections |
| M6 | complete | [`milestones/m6-offline-downloads.md`](milestones/m6-offline-downloads.md) | Playlist downloaded; airplane-mode playback works |
| M7 | complete | [`milestones/m7-sync-and-write-replay.md`](milestones/m7-sync-and-write-replay.md) | Offline likes/edits drain to server in clock order, idempotent |
| M8 | complete | [`milestones/m8-websocket-and-polish.md`](milestones/m8-websocket-and-polish.md) | Live now-playing push; optional crossfade |
| M9 | complete | [`milestones/m9-secure-enrollment.md`](milestones/m9-secure-enrollment.md) | Public device registration is locked behind owner setup, admin login, and one-time pairing codes |
| M10 | complete | [`milestones/m10-admin-dashboard.md`](milestones/m10-admin-dashboard.md) | Browser admin dashboard manages pairing, devices, scans, cookies, and now-playing control |
| — | **visually verified** | [`client-verification-report.md`](client-verification-report.md) | 22 goldens (PR) + 10 smoke artifacts (nightly Android) cover M1–M8 |

Order rationale: InnerTube (M3) must precede recs (M5) because recs depend on
it. Offline (M6) and full sync (M7) come late because they need the rest of
the system stable to be tested honestly. M9 and M10 are post-v1 hardening and
operations milestones: M9 fixes enrollment/auth boundaries first, then M10
builds the admin dashboard on those boundaries.

## Post-v1 hardening direction

M0-M8 prove the music system. M9-M10 make it safe and operable:

- **M9 Secure Enrollment** replaces open device self-registration with owner
  setup, admin sessions, one-time pairing codes, device revocation, rate
  limiting, and audit events.
- **M10 Admin Dashboard** turns the M8 JSON admin surface into a real
  server-served web dashboard for devices, pairing, library scans, YouTube
  cookie health, and now-playing control.

Local mode is part of the Rust rewrite rather than a separate post-M10 desktop
deferment. The Flutter client opens a device-local SQLite store through Flutter
Rust Bridge, records playback/preference/impression signals there, ranks local
candidates with `sunflower-core`, and drains remote-contract feedback from the
Rust SQLite event log to the configured recommendation server once it is
reachable.

## V1 client — visual verification

M0–M10 acceptance criteria are now verified on the Rust implementation. The
Rust workspace is verified with `cargo test --workspace --locked`, contract
tests that preserve the established wire shapes and selected behavior,
`cargo clippy --workspace --locked --all-targets -- -D warnings`, and
`cargo fmt --all -- --check`; Postgres-backed Rust parity is run with
`just test-rust-pg-local` and by the `rust-parity` CI job. The Flutter client
was subsequently brought up to the same standard:

- **22 golden-test baselines** (`client/test/goldens/goldens/snapshots/`) cover every screen and
  key state from the [verification matrix](post-v1-visual-verification.md). Pixel-diff
  regression runs on every PR via the `golden-tests` job in
  `.github/workflows/client-verify.yml`.
- **10 Android emulator smoke artifacts** (`client/build/smoke-artifacts/t0N_*`)
  walk the full M1–M8 demo flow on a live AVD nightly. Captured screenshots are
  uploaded as CI artifacts and map 1-to-1 to milestone acceptance criteria in
  [`client-verification-report.md`](client-verification-report.md).

**The V1 Flutter client is visually verified.** Deferred items (iOS simulator, YT-cookie
home feed, two-device WS concurrency in CI) are documented in the report's coverage-gaps
table and are out of scope for V1.

## Client/runtime boundary — honest framing

The client is no longer "no recommendation logic". It owns:

- `just_audio` / `audio_service` player + OS media session
- Drift SQLite database (pending mutations, lookahead cache, downloaded
  tracks, home cache, recent plays)
- Background isolate download manager
- Write-replay buffer for offline mutations
- Flutter Rust Bridge handle to `sunflower-core` + SQLite local recommendation
  state
- Optional standalone recommendation server URL; Home reads and local feedback
  drain to it, falling back to the main server when unset
- Local-home fallback ranking from recent plays, downloads, likes, impressions,
  and playback completions

It does NOT own:

- InnerTube calls or YouTube cookie custody
- Cross-device merge/source-of-truth resolution
- Server queue/session ownership while online
- Canonical remote recommendation cache and remote section fan-out
