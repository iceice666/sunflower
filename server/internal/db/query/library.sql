-- name: UpsertArtist :one
INSERT INTO artists (media_id, source_type, name, raw_metadata)
VALUES (@media_id, @source_type, @name, @raw_metadata)
ON CONFLICT (media_id) DO UPDATE SET
    name         = EXCLUDED.name,
    raw_metadata = EXCLUDED.raw_metadata
RETURNING *;

-- name: UpsertAlbum :one
INSERT INTO albums (media_id, source_type, title, primary_artist_id, year, raw_metadata)
VALUES (@media_id, @source_type, @title, @primary_artist_id, @year, @raw_metadata)
ON CONFLICT (media_id) DO UPDATE SET
    title             = EXCLUDED.title,
    primary_artist_id = EXCLUDED.primary_artist_id,
    year              = EXCLUDED.year,
    raw_metadata      = EXCLUDED.raw_metadata
RETURNING *;

-- name: UpsertSong :one
INSERT INTO songs (media_id, source_type, title, duration_ms, album_id, primary_artist_id, raw_metadata, local_path)
VALUES (@media_id, @source_type, @title, @duration_ms, @album_id, @primary_artist_id, @raw_metadata, @local_path)
ON CONFLICT (media_id) DO UPDATE SET
    title             = EXCLUDED.title,
    duration_ms       = EXCLUDED.duration_ms,
    album_id          = EXCLUDED.album_id,
    primary_artist_id = EXCLUDED.primary_artist_id,
    raw_metadata      = EXCLUDED.raw_metadata,
    local_path        = EXCLUDED.local_path
RETURNING *;

-- name: UpsertSongArtist :exec
INSERT INTO song_artists (song_media_id, artist_media_id, position)
VALUES (@song_media_id, @artist_media_id, @position)
ON CONFLICT (song_media_id, artist_media_id) DO UPDATE SET
    position = EXCLUDED.position;

-- name: GetSongStream :one
SELECT media_id, local_path FROM songs WHERE media_id = @media_id;

-- name: ListSongs :many
SELECT
    s.media_id,
    s.source_type,
    s.title,
    s.duration_ms,
    s.album_id,
    s.primary_artist_id,
    s.explicit,
    s.video_only,
    s.available,
    s.loudness_db,
    s.last_resolved_at,
    s.raw_metadata,
    COALESCE(ar.name, '')    AS artist_name,
    COALESCE(al.title, '')   AS album_title,
    (s.album_id IS NOT NULL) AS has_art
FROM songs s
LEFT JOIN artists ar ON ar.media_id = s.primary_artist_id
LEFT JOIN albums  al ON al.media_id = s.album_id
WHERE s.available = true
ORDER BY s.title
LIMIT @page_size OFFSET @page_offset;

-- name: ListAlbums :many
SELECT * FROM albums
WHERE available = true
ORDER BY title
LIMIT @page_size OFFSET @page_offset;

-- name: ListArtists :many
SELECT * FROM artists
WHERE available = true
ORDER BY name
LIMIT @page_size OFFSET @page_offset;
