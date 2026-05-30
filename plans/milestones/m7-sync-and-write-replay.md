# M7 — Full Sync + Write-Replay

## Demo target

- Airplane mode on. In the app: like 5 songs, unlike 1, add 3 songs to a
  playlist, reorder 2, scrobble 4 play_events.
- Reconnect.
- Within seconds, all changes have reached the server in `client_clock` order;
  retries are idempotent (replaying a confirmed mutation does NOT double-apply);
  cross-device conflicts resolved by last-write-wins on `occurred_at`.
- UI shows "N pending" indicator that drops to 0 once drained.

## Scope

- Server: `Idempotency-Key` middleware on every mutating route.
- Server: `idempotency_log` GC.
- Server: conflict resolution returns `{accepted, server_state}` for each
  mutation so the client can reconcile.
- Client: `PendingMutations` Drift table with status state machine.
- Client: write-replay buffer with exponential backoff (5 s → 30 s → 5 min →
  30 min → 2 h cap).
- Client: 10 000-entry buffer cap with drop-oldest-non-confirmed; likes win
  over impressions on eviction.
- Client: every mutating call goes through the buffer first.
- UI: pending-sync indicator with a count and a manual "retry now" action.

## Files to create

```
server/internal/sync/
  idempotency.go             # middleware: read Idempotency-Key, dedupe, replay
  conflict.go                # last-write-wins by occurred_at
  gc.go                      # periodic purge of expired idempotency_log rows
server/internal/events/
  ingest.go                  # POST /events with batched idempotency keys
  scrobble_window.go         # mirrors Metrolist's PlaybackStatsListener threshold
server/internal/api/
  handlers_events.go         # POST /events
  middleware_idempotency.go  # wires sync.Idempotency into the router
server/db/queries/
  sync.sql                   # InsertIdempotencyLog, FindIdempotencyLog, GC

client/lib/core/sync/
  replay_buffer.dart         # public API: enqueueMutation, drain, status
  pending_mutation.dart      # entity + status enum
  retry_policy.dart          # exponential backoff
  eviction.dart              # buffer-cap policy
  idempotency_key.dart       # UUIDv7 generator
client/lib/core/api/
  api_client.dart            # wrapped: every mutating call → buffer first
client/lib/features/settings/
  sync_status_widget.dart    # pending count + retry button
```

## Acceptance criteria

- `Idempotency-Key` middleware applied to: `/likes`, `/playlists/*`,
  `/events`, `/queue/*/mutate`, `/streams/resolve`, `/cookies/youtube`,
  `/devices/{id}/downloads`.
- Replaying the same request body with the same key within 24 h returns the
  same response without applying the mutation a second time.
- Buffered mutations are replayed in `client_clock` ASC.
- Two devices liking the same track at slightly different times resolves to
  the later `occurred_at`; the earlier device sees its local state corrected
  on next refresh.
- Buffer overflow test: enqueue 10 001 entries → oldest non-confirmed
  impression is evicted; counter `buffer_overflow_drops` increments;
  surfaces in `sync_status_widget`.
- Idempotency log GC removes rows older than 24 h hourly.

## Dependencies on prior milestones

- All prior milestones — this is the sync layer that wraps every mutation
  defined so far.

## Verification

- Server unit tests: same key replayed → identical response; same payload +
  different key → both apply.
- Server conflict resolver tests for likes and playlist edits.
- Client replay-buffer test: 20 mutations queued offline → online → all
  drained in clock order → re-replay of 10 produces no duplicates server-side.
- Client retry-policy test: backoff sequence matches spec.
- Client eviction test: insert 10 001 entries; verify drop priority.
- Integration test: docker-compose stack; client goes offline; mutations made;
  reconnect; server state matches expected.

## Out of M7 scope

- Real-time push of remote mutations TO the client — for now, the client
  pulls (`If-None-Match`) on refresh / app resume; live push is M8 for
  now-playing only.
