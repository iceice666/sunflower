-- name: MostPlayedSongs :many
-- Top songs by play count in the trailing window, newest-tie-broken. Powers
-- Quick Picks (local-first) and seed selection.
SELECT
    pe.song_media_id,
    COUNT(*)               AS play_count,
    MAX(pe.occurred_at)    AS last_played_at,
    COALESCE(s.title, '')  AS title,
    COALESCE(ar.name, '')  AS artist_name,
    s.album_id,
    s.duration_ms,
    s.source_type,
    s.explicit,
    s.video_only
FROM play_events pe
JOIN songs s ON s.media_id = pe.song_media_id
LEFT JOIN artists ar ON ar.media_id = s.primary_artist_id
WHERE pe.user_id = @user_id
  AND pe.occurred_at > @since
  AND s.available = true
GROUP BY pe.song_media_id, s.title, ar.name, s.album_id, s.duration_ms,
         s.source_type, s.explicit, s.video_only
ORDER BY play_count DESC, last_played_at DESC
LIMIT @page_size;

-- name: MostPlayedArtists :many
-- Top artists by aggregate play count in the trailing window. Powers the
-- "Similar to <artist>" rows (one per top-N artist).
SELECT
    s.primary_artist_id   AS artist_id,
    COALESCE(ar.name, '') AS artist_name,
    COUNT(*)              AS play_count
FROM play_events pe
JOIN songs s ON s.media_id = pe.song_media_id
LEFT JOIN artists ar ON ar.media_id = s.primary_artist_id
WHERE pe.user_id = @user_id
  AND pe.occurred_at > @since
  AND s.primary_artist_id IS NOT NULL
GROUP BY s.primary_artist_id, ar.name
ORDER BY play_count DESC
LIMIT @page_size;

-- name: ForgottenFavorites :many
-- Songs played a lot historically but not recently — surfaced in Quick Picks
-- to re-introduce neglected favorites.
SELECT
    pe.song_media_id,
    COUNT(*)              AS play_count,
    MAX(pe.occurred_at)   AS last_played_at,
    COALESCE(s.title, '') AS title,
    COALESCE(ar.name, '') AS artist_name,
    s.album_id,
    s.duration_ms,
    s.source_type
FROM play_events pe
JOIN songs s ON s.media_id = pe.song_media_id
LEFT JOIN artists ar ON ar.media_id = s.primary_artist_id
WHERE pe.user_id = @user_id
  AND s.available = true
GROUP BY pe.song_media_id, s.title, ar.name, s.album_id, s.duration_ms,
         s.source_type
HAVING MAX(pe.occurred_at) < @stale_before
ORDER BY play_count DESC
LIMIT @page_size;

-- name: RecentImpressionMediaIDs :many
-- media_ids shown to the user since @since — the novelty / dedupe filter input.
SELECT media_id, COUNT(*) AS shows
FROM recommendation_impressions
WHERE user_id = @user_id
  AND media_id IS NOT NULL
  AND shown_at > @since
GROUP BY media_id;

-- name: InsertImpression :exec
INSERT INTO recommendation_impressions
    (user_id, section_id, source, seed_id, media_id, position)
VALUES (@user_id, @section_id, @source, @seed_id, @media_id, @position);

-- name: UpsertLike :one
-- Idempotent like insert. The idempotency_key (UUIDv7) dedupes replays of the
-- same client mutation; the (user, song) PK dedupes logical likes.
INSERT INTO likes (user_id, song_media_id, liked_at, idempotency_key)
VALUES (@user_id, @song_media_id, @liked_at, @idempotency_key)
ON CONFLICT (user_id, song_media_id) DO UPDATE SET
    liked_at = GREATEST(likes.liked_at, EXCLUDED.liked_at)
RETURNING *;

-- name: DeleteLike :exec
DELETE FROM likes WHERE user_id = @user_id AND song_media_id = @song_media_id;

-- name: IsLiked :one
SELECT EXISTS (
    SELECT 1 FROM likes WHERE user_id = @user_id AND song_media_id = @song_media_id
) AS liked;

-- name: ListLikes :many
SELECT song_media_id, liked_at FROM likes
WHERE user_id = @user_id
ORDER BY liked_at DESC
LIMIT @page_size OFFSET @page_offset;

-- name: InsertPlaylist :one
INSERT INTO playlists (user_id, title, source_type, external_id)
VALUES (@user_id, @title, @source_type, @external_id)
RETURNING *;

-- name: GetPlaylist :one
SELECT * FROM playlists WHERE id = @id AND user_id = @user_id;

-- name: ListPlaylists :many
SELECT * FROM playlists
WHERE user_id = @user_id
ORDER BY created_at DESC
LIMIT @page_size OFFSET @page_offset;

-- name: UpdatePlaylistTitle :one
UPDATE playlists
SET title = @title, version = version + 1
WHERE id = @id AND user_id = @user_id
RETURNING *;

-- name: DeletePlaylist :exec
DELETE FROM playlists WHERE id = @id AND user_id = @user_id;

-- name: AddPlaylistItem :exec
-- Append a song at the next position. position is computed by the caller as
-- COALESCE(MAX(position)+1, 0) via NextPlaylistPosition.
INSERT INTO playlist_items (playlist_id, position, song_media_id, added_by_device_id)
VALUES (@playlist_id, @position, @song_media_id, @added_by_device_id)
ON CONFLICT (playlist_id, position) DO NOTHING;

-- name: NextPlaylistPosition :one
SELECT COALESCE(MAX(position) + 1, 0)::int AS next_position
FROM playlist_items WHERE playlist_id = @playlist_id;

-- name: ListPlaylistItems :many
SELECT
    pi.position,
    pi.song_media_id,
    COALESCE(s.title, '')  AS title,
    COALESCE(ar.name, '')  AS artist_name,
    s.album_id,
    s.duration_ms
FROM playlist_items pi
JOIN songs s ON s.media_id = pi.song_media_id
LEFT JOIN artists ar ON ar.media_id = s.primary_artist_id
WHERE pi.playlist_id = @playlist_id
ORDER BY pi.position;

-- name: RemovePlaylistItem :exec
DELETE FROM playlist_items
WHERE playlist_id = @playlist_id AND song_media_id = @song_media_id;

-- name: BumpPlaylistVersion :exec
UPDATE playlists SET version = version + 1 WHERE id = @id AND user_id = @user_id;

-- name: GetRecCache :one
SELECT cache_key, payload, generated_at, expires_at
FROM rec_cache
WHERE cache_key = @cache_key;

-- name: UpsertRecCache :exec
INSERT INTO rec_cache (cache_key, user_id, payload, generated_at, expires_at)
VALUES (@cache_key, @user_id, @payload, now(), @expires_at)
ON CONFLICT (cache_key) DO UPDATE SET
    payload      = EXCLUDED.payload,
    generated_at = EXCLUDED.generated_at,
    expires_at   = EXCLUDED.expires_at;
