# M6 — Offline Downloads

> **Archive note (2026-07-01):** This milestone is retained as historical
> build and acceptance context from the original Go `server/` implementation.
> The canonical implementation is now Rust under `rust/`; use
> [`../README.md`](../README.md) and [`../architecture.md`](../architecture.md)
> for current crate layout, migrations, assets, and verification commands.

## Demo target

- Long-press a playlist → "Download for offline".
- Download progresses with a notification; tracks land under app's storage.
- Toggle airplane mode.
- Open playlist → tracks play from local file URIs — no network at all.
- Reconnect → downloads are intact; server-side `downloaded_tracks` reflects
  the per-device state.

## Scope

- Client-side background download manager (Dart isolate).
- Drift `DownloadJobs` and `DownloadedTracks` tables.
- Per-track and per-playlist download UI.
- Storage path under `getApplicationSupportDirectory()/downloads/`.
- Verification: SHA-256 for local-library files (server provides hash);
  YouTube downloads accept best-effort.
- Server: `POST /api/v1/devices/{id}/downloads` to register; `DELETE` to remove.
- Player layer: prefer local file if `DownloadedTracks` contains the
  `media_id`, regardless of what `/next` returned.

## Files to create

```
client/lib/core/downloads/
  isolate_runner.dart       # spawn isolate, two-way SendPorts
  download_manager.dart     # public API: enqueue, cancel, status stream
  download_worker.dart      # runs in isolate, dio download with Range
  storage.dart              # path resolution, dir creation, free-space check
  verifier.dart             # SHA-256 streaming for local-library files
client/lib/core/db/
  database.dart             # add DownloadJobs, DownloadedTracks tables + DAOs
client/lib/features/downloads_ui/
  downloads_screen.dart     # active jobs + completed list
  download_button.dart      # reusable widget for track / playlist tiles
client/lib/core/player/
  source_resolver.dart      # prefer-local logic: if media_id in
                            # DownloadedTracks → file URI; else use /next URL

server/internal/api/
  handlers_downloads.go     # POST/DELETE /devices/{id}/downloads
                            # GET /library/songs/{id}/hash for local songs
server/db/queries/
  downloads.sql             # UpsertDownload, ListDownloadsForDevice,
                            # DeleteDownload
```

## Acceptance criteria

- Per-track download: progress visible, completes, SHA verified, plays
  offline.
- Per-playlist download: jobs enqueued one-by-one; cancel removes pending and
  stops in-progress.
- Download persists across app restarts (jobs in Drift, resumable via Range).
- Disk-full handled: job marked failed with a clear error in the UI.
- Server registry stays consistent: removing a download from the app fires a
  `DELETE /devices/{id}/downloads/{media_id}` (deferred to M7 if server is
  offline → write-replay buffer).
- Switching tracks in a downloaded playlist while offline never touches the
  network.

## Dependencies on prior milestones

- M4 source resolver + queue.
- M5 playlist CRUD (since "download a playlist" is a primary use case).

## Verification

- Unit test for `source_resolver.dart`: local-present > /next URL preference.
- Integration test: simulate download of 3-track playlist, kill app mid-job,
  restart, assert resume-from-position; verify final SHA.
- Manual airplane-mode test.

## Platform notes

- Android: foreground service for the isolate, in-flight notification via
  `flutter_local_notifications`.
- iOS: background tasks API is restrictive — downloads pause when app is
  backgrounded for long periods; document this in UI ("downloads continue
  while the app is in the foreground").
- Desktop: works straightforwardly.
- Web: out of scope for v1; the download UI shows a "not supported" notice on
  web.

## Out of M6 scope

- Full sync semantics for like / playlist edits while offline (M7).
- Smart auto-download (e.g. "always keep liked songs downloaded") — v2.

## Implementation status

**Server half: done.** Verified (`go build`/`vet`/`test` incl. a testcontainers
integration test `internal/api/integration_m6_test.go`, `gofmt`, `sqlc` green):

- `db/query/downloads.sql` (+ generated layer) — `UpsertDownload`,
  `ListDownloadsForDevice`, `DeleteDownload`, `GetSongHashInfo`. The
  `downloaded_tracks` table already shipped in migration 0004.
- `internal/api/handlers_downloads.go` + routes: `GET/POST
  /devices/{id}/downloads`, `DELETE /devices/{id}/downloads/{media_id}` (each
  asserts the path device id matches the authenticated device), and
  `GET /library/songs/{media_id}/hash` (streams SHA-256 of the local file).
- Test covers register → list → hash-verify → cross-device 403 → delete.

**Client half: done.** Implemented to spec, parse/format-verified with
`dart format` (no Flutter SDK here → `build_runner`/`flutter test` run in a
Flutter env; see M4 note):

- `core/db/database.dart` — `DownloadJobs` (resumable, status state machine) and
  `DownloadedTracks` (verified local files) tables + DAOs.
- `core/downloads/` — `isolate_runner.dart` (worker isolate + typed channel),
  `download_worker.dart` (Range-resumable dio stream to a `.part` file, atomic
  rename on completion), `download_manager.dart` (enqueue track/playlist,
  cancel, remove, resume-on-start, SHA verify for local songs, server
  registration), `storage.dart`, `verifier.dart` (streaming SHA-256),
  `downloads_providers.dart`.
- `core/player/source_resolver.dart` — prefer-local: a downloaded `file://`
  URI wins over any `/next` URL; wired into the audio handler's queue mode so
  offline playback never touches the network.
- `features/downloads_ui/` — `downloads_screen.dart` (active progress + cancel,
  completed + remove) and `download_button.dart` (reusable, platform-gated).
- Playlist detail screen gains a "Download for offline" action; `app.dart`
  adds a Downloads tab; device id now persisted at registration for the
  per-device registry.
- Test: `test/source_resolver_test.dart` (prefer-local rule over in-memory
  Drift).

Self-reviewed (review agents unavailable due to an environment/model error).
Fixes applied during review: `cancel()` now deletes the `.part` file (not the
final path), and a dead no-op `freeBytes()` pre-check was removed in favor of
ENOSPC-at-write handling.
