# M0 — Server Bootstrap

> **Archive note (2026-07-01):** This milestone is retained as historical
> build and acceptance context from the original Go `server/` implementation.
> The canonical implementation is now Rust under `rust/`; use
> [`../README.md`](../README.md) and [`../architecture.md`](../architecture.md)
> for current crate layout, migrations, assets, and verification commands.

## Demo target

`just run` boots the Rust `sunflowerd`, connects to a local Postgres, applies
migrations, and `curl http://localhost:8080/healthz` returns `200 {"status":"ok"}`.

## Scope

Skeleton only. No business logic. The goal is a runnable binary and a
migration framework so M1+ can land schema and handlers without scaffolding
debt.

## Files to create

```
server/
  go.mod
  go.sum
  cmd/sunflowerd/main.go         # boot, config load, router, ListenAndServe
  internal/api/router.go         # chi router setup, healthz route
  internal/api/middleware.go     # request-id, logging, recover, CORS
  internal/db/db.go              # pgx pool, dialer, ping
  internal/config/config.go      # env-based config (DATABASE_URL, LISTEN_ADDR, …)
  db/migrations/0001_init.sql    # users, devices, songs, albums, artists,
                                 # song_artists, playlists, playlist_items
  db/migrations/0002_events.sql  # play_events, likes, recommendation_impressions
  db/migrations/0003_queue.sql   # queue_sessions, queue_items
  db/migrations/0004_sync.sql    # idempotency_log, rec_cache, encrypted_cookies,
                                 # downloaded_tracks
dev/
  docker-compose.yml             # postgres:16 with persistent volume
  justfile                       # up, down, migrate, sqlc, test
```

Schema content lives in [`../architecture.md`](../architecture.md#postgres-schema-key-tables).

## Acceptance criteria

- `just dev-up` brings Postgres up on `localhost:5432`.
- `just migrate` runs goose, exits 0, leaves all M0 tables present.
- `just run` starts the server; `curl /healthz` returns 200.
- `sqlc generate` runs clean against the migrations (even if no queries yet).
- `go test ./...` passes (only smoke tests in M0).

## Dependencies

- Go 1.22+
- Postgres 16
- `goose` for migrations
- `sqlc` for query generation
- `chi/v5` for routing
- `pgx/v5` for the database driver
- `zerolog` for structured logging

## Verification

- Smoke test in `cmd/sunflowerd/main_test.go` that boots the router and asserts
  `/healthz` responds.
- Migration round-trip test in `internal/db/db_test.go` using
  `testcontainers-go` Postgres: apply all migrations up, then all down, assert
  no orphans.

## Out of M0 scope

- Authentication (M1)
- Any `/api/v1` endpoint beyond `/healthz`
- Any background jobs
- TLS / reverse-proxy config (run behind Caddy/nginx later)
