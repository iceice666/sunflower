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
INSERT INTO songs (media_id, source_type, title, duration_ms, album_id, primary_artist_id, raw_metadata)
VALUES (@media_id, @source_type, @title, @duration_ms, @album_id, @primary_artist_id, @raw_metadata)
ON CONFLICT (media_id) DO UPDATE SET
    title             = EXCLUDED.title,
    duration_ms       = EXCLUDED.duration_ms,
    album_id          = EXCLUDED.album_id,
    primary_artist_id = EXCLUDED.primary_artist_id,
    raw_metadata      = EXCLUDED.raw_metadata
RETURNING *;

-- name: UpsertSongArtist :exec
INSERT INTO song_artists (song_media_id, artist_media_id, position)
VALUES (@song_media_id, @artist_media_id, @position)
ON CONFLICT (song_media_id, artist_media_id) DO UPDATE SET
    position = EXCLUDED.position;

-- name: ListSongs :many
SELECT * FROM songs
WHERE available = true
ORDER BY title
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
