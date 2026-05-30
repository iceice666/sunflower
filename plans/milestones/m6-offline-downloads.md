# M6 — Offline Downloads

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
