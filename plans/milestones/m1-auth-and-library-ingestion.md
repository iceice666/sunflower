# M1 — Auth + Library Ingestion

## Demo target

```
curl -X POST localhost:8080/api/v1/auth/register-device \
  -d '{"device_name":"laptop","platform":"linux","client_version":"0.1"}'
→ {"device_id":"…","token":"sf_dev_…","server_capabilities":[…]}

curl -X POST localhost:8080/api/v1/library/scan \
  -H "Authorization: Bearer sf_dev_…" \
  -d '{"roots":["/music"]}'
→ {"job_id":"…"}

curl localhost:8080/api/v1/library/songs?limit=20 -H "Authorization: …"
→ {"songs":[{"media_id":"local:…","title":"…",…}, …]}
```

A music folder is scanned, tags extracted, songs/albums/artists populated in
Postgres, and accessible via authenticated REST.

## Scope

- Device registration and token middleware (`internal/auth`).
- Library scanner (`internal/library`) with fsnotify watcher.
- Tag extraction with `dhowden/tag`.
- Cover art thumbnail generation (256/512/1024).
- Library CRUD handlers (songs, albums, artists; playlists in M5).
- Background job framework (`internal/jobs`) with `GET /api/v1/jobs/{id}` polling.

## Files to create

```
server/internal/auth/
  device.go              # registration handler, token issuance (random + argon2id)
  middleware.go          # Bearer-token middleware → injects user_id, device_id
server/internal/library/
  scanner.go             # walk roots, fsnotify watch, debounce, dispatch
  tags.go                # dhowden/tag wrapper, media_id derivation
  artwork.go             # extract + resize via disintegration/imaging
  repo.go                # sqlc-generated query layer for songs/albums/artists
server/internal/jobs/
  registry.go            # in-memory job store (M1 only; can move to DB later)
  scan_job.go            # library scan job impl
server/internal/api/
  handlers_auth.go       # register-device handler
  handlers_library.go    # GET /songs, /albums, /artists, POST /library/scan
  handlers_jobs.go       # GET /jobs/{id}
server/db/queries/
  library.sql            # sqlc queries: ListSongs, ListAlbums, ListArtists,
                         # UpsertSong, UpsertAlbum, UpsertArtist
```

## Acceptance criteria

- Device registration returns a token; subsequent calls without
  `Authorization` return 401.
- `POST /api/v1/library/scan` enqueues a job; `GET /api/v1/jobs/{id}` shows
  progress (`pending → running → completed`) with `processed_files` count.
- After a scan of a folder with 50 mixed MP3/FLAC/M4A files, all 50 songs are
  in the `songs` table with `source_type='local'`.
- Cover art for each album exists at
  `<data>/art/<album_media_id>/{256,512,1024}.jpg`.
- Modifying a file in the watched folder triggers an upsert within 5 s.
- Token middleware injects `user_id` and `device_id` into request context,
  used by all subsequent handlers.

## Dependencies on prior milestones

- M0 server bootstrap, migrations applied.

## Verification

- Unit tests: tag extraction against fixture files (one per format) under
  `internal/library/testdata/`.
- Unit test for `media_id` stability across rescans (same path → same id).
- Integration test: spin a temp directory, drop in 3 files, run scan, assert
  rows present; modify one file's tags, assert upsert.
- HTTP test: register a device, scan a fixture folder, list songs.

## Out of M1 scope

- Playlist CRUD (M5; depends on UI flow design).
- Likes (M5).
- YouTube catalog (M3+).
- Multi-user (out of scope for v1 entirely).
