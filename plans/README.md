# Sunflower — Plan

A Metrolist-inspired self-hosted music system in two parts: a Flutter
cross-platform client and a Go server that owns recommendations, history,
preferences, and the "what plays next" decision.

## Why this plan exists

The research in [`../docs/`](../docs/) describes Metrolist's Android-only
architecture — Media3/ExoPlayer client, on-device Room DB, on-device InnerTube
fan-out. This plan turns that research into a two-artifact system where the
client is slim *in logic* (no recommendation generation, no direct InnerTube
calls) while the server takes on the brain. The client still has real surface
area — player, OS media session, Drift SQLite, offline downloads, write-replay
buffer for offline mutations — but every "what plays next" decision originates
on the server.

## Locked decisions

| Area | Choice |
|---|---|
| Client | Flutter (iOS, Android, desktop, web), `just_audio` + `audio_service`, Drift |
| Catalog | Hybrid: self-hosted library files + YouTube Music fallback |
| Stream delivery | Client direct-to-origin by default; server proxy as fallback on 403/CORS |
| Server stack | Go + chi + Postgres |
| InnerTube | Reimplemented natively in Go (no Python sidecar) |
| YT personalization | Server holds user's YT cookies, encrypted at rest (libsodium) |
| Sync | Server is source of truth; client buffers writes offline and replays; now-playing pushed via WebSocket; rest is polling |
| Recommendations | Mirror Metrolist's InnerTube-derived fan-out server-side (no ML) |
| Offline | Explicit per-track / per-playlist downloads with local cache + DB |
| Auth | Single-user, long-lived opaque device tokens (no refresh) |

## How to read this folder

- [`architecture.md`](architecture.md) — static reference: component map, wire
  protocol shapes, Postgres schema, server-internal designs, client-internal
  designs. Read this once; refer back during every milestone.
- [`milestones/`](milestones/) — one file per build phase (M0–M8). Each has a
  demo target, scope, file-level acceptance criteria, and verification steps.
  Work them in order — later milestones assume earlier ones are stable.
- [`risks.md`](risks.md) — top risks with mitigations, plus what is explicitly
  out of scope for v1.

## Milestone index

| # | Status | File | Demo target |
|---|---|---|---|
| M0 | complete | [`milestones/m0-server-bootstrap.md`](milestones/m0-server-bootstrap.md) | `sunflowerd` boots, `/healthz` returns OK, migrations applied |
| M1 | complete | [`milestones/m1-auth-and-library-ingestion.md`](milestones/m1-auth-and-library-ingestion.md) | Device registers, library scan populates songs/albums/artists |
| M2 | — | [`milestones/m2-flutter-player-local-library.md`](milestones/m2-flutter-player-local-library.md) | Flutter app plays a local-library track end-to-end |
| M3 | — | [`milestones/m3-innertube-client.md`](milestones/m3-innertube-client.md) | `probe innertube next --video-id=…` returns a fresh playable URL |
| M4 | — | [`milestones/m4-next-endpoint-and-lookahead.md`](milestones/m4-next-endpoint-and-lookahead.md) | Client plays YT tracks with `/next` lookahead and 403 re-resolve |
| M5 | — | [`milestones/m5-recommendation-pipeline.md`](milestones/m5-recommendation-pipeline.md) | Home feed populated; cold-start renders cached sections |
| M6 | — | [`milestones/m6-offline-downloads.md`](milestones/m6-offline-downloads.md) | Playlist downloaded; airplane-mode playback works |
| M7 | — | [`milestones/m7-sync-and-write-replay.md`](milestones/m7-sync-and-write-replay.md) | Offline likes/edits drain to server in clock order, idempotent |
| M8 | — | [`milestones/m8-websocket-and-polish.md`](milestones/m8-websocket-and-polish.md) | Live now-playing push; optional crossfade |

Order rationale: InnerTube (M3) must precede recs (M5) because recs depend on
it. Offline (M6) and full sync (M7) come last because they need the rest of
the system stable to be tested honestly.

## "Slim client" — honest framing

The client is slim in *logic*, not surface area. It owns:

- `just_audio` / `audio_service` player + OS media session
- Drift SQLite database (pending mutations, lookahead cache, downloaded
  tracks, home cache, recent plays)
- Background isolate download manager
- Write-replay buffer for offline mutations

It does NOT own:

- Candidate generation or ranking
- InnerTube calls
- YT cookies
- Cross-device merge logic (server resolves with last-write-wins)
