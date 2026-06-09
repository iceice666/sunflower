-- name: InsertUser :one
INSERT INTO users (display_name)
VALUES (@display_name)
RETURNING *;

-- name: GetFirstUser :one
SELECT * FROM users ORDER BY created_at LIMIT 1;

-- name: InsertDevice :one
INSERT INTO devices (user_id, name, platform, token_hash)
VALUES (@user_id, @name, @platform, @token_hash)
RETURNING *;

-- name: GetDeviceByTokenHash :one
SELECT * FROM devices WHERE token_hash = @token_hash;

-- name: UpdateDeviceLastSeen :exec
UPDATE devices SET last_seen_at = now() WHERE id = @id;
