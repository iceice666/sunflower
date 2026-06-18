-- name: UpsertDownload :one
-- Registers (or updates) a per-device downloaded track. Idempotent on the
-- (device, song) primary key so a replayed registration is a no-op update.
INSERT INTO downloaded_tracks
    (device_id, song_media_id, local_path, bytes, completed_at, last_verified_at)
VALUES (@device_id, @song_media_id, @local_path, @bytes, @completed_at, @last_verified_at)
ON CONFLICT (device_id, song_media_id) DO UPDATE SET
    local_path       = EXCLUDED.local_path,
    bytes            = EXCLUDED.bytes,
    completed_at     = EXCLUDED.completed_at,
    last_verified_at = EXCLUDED.last_verified_at
RETURNING *;

-- name: ListDownloadsForDevice :many
SELECT device_id, song_media_id, local_path, bytes, completed_at, last_verified_at
FROM downloaded_tracks
WHERE device_id = @device_id
ORDER BY completed_at DESC NULLS LAST;

-- name: DeleteDownload :exec
DELETE FROM downloaded_tracks
WHERE device_id = @device_id AND song_media_id = @song_media_id;

-- name: GetSongHashInfo :one
-- Returns the local path of a song so the server can compute (and the client
-- can verify) a SHA-256 for local-library downloads.
SELECT media_id, source_type, local_path
FROM songs
WHERE media_id = @media_id;
