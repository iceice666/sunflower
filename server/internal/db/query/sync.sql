-- name: FindIdempotencyLog :one
-- Looks up a prior request by its idempotency key. A hit means the mutation was
-- already applied; the caller replays the stored response without re-applying.
SELECT key, user_id, device_id, route, response_hash, created_at, expires_at
FROM idempotency_log
WHERE key = @key;

-- name: InsertIdempotencyLog :exec
-- Records that a mutation with this key was applied, with the response hash for
-- replay and an expiry for GC. ON CONFLICT DO NOTHING so a racing duplicate
-- insert is a no-op (the first writer wins).
INSERT INTO idempotency_log (key, user_id, device_id, route, response_hash, expires_at)
VALUES (@key, @user_id, @device_id, @route, @response_hash, @expires_at)
ON CONFLICT (key) DO NOTHING;

-- name: GCIdempotencyLog :execrows
-- Deletes expired idempotency rows. Returns the number removed.
DELETE FROM idempotency_log WHERE expires_at IS NOT NULL AND expires_at < @cutoff;

-- name: InsertPlayEvent :exec
-- Records a play event from the batched /events endpoint.
INSERT INTO play_events
    (user_id, device_id, song_media_id, queue_id, kind, occurred_at, total_played_ms, reason)
VALUES (@user_id, @device_id, @song_media_id, @queue_id, @kind, @occurred_at, @total_played_ms, @reason);
