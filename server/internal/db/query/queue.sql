-- name: InsertQueueSession :one
INSERT INTO queue_sessions (user_id, device_id, seed_kind, seed_id, title, items)
VALUES (@user_id, @device_id, @seed_kind, @seed_id, @title, @items)
RETURNING *;

-- name: GetQueueSession :one
SELECT * FROM queue_sessions WHERE id = @id AND user_id = @user_id;

-- name: InsertQueueItem :exec
INSERT INTO queue_items (queue_id, position, media_id, source_data)
VALUES (@queue_id, @position, @media_id, @source_data)
ON CONFLICT (queue_id, position) DO UPDATE SET
    media_id    = EXCLUDED.media_id,
    source_data = EXCLUDED.source_data;

-- name: ListQueueItems :many
SELECT queue_id, position, media_id, source_data
FROM queue_items
WHERE queue_id = @queue_id
ORDER BY position;

-- name: ListQueueItemsFrom :many
SELECT queue_id, position, media_id, source_data
FROM queue_items
WHERE queue_id = @queue_id AND position >= @from_position
ORDER BY position
LIMIT @page_size;

-- name: ListLikedSongs :many
SELECT s.media_id, s.title, s.duration_ms
FROM likes l
JOIN songs s ON s.media_id = l.song_media_id
WHERE l.user_id = @user_id AND s.available = true
ORDER BY l.liked_at DESC
LIMIT @page_size;
