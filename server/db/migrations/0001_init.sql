-- +goose Up
-- +goose StatementBegin

CREATE TABLE users (
    id           uuid        NOT NULL DEFAULT gen_random_uuid() PRIMARY KEY,
    display_name text        NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE devices (
    id           uuid        NOT NULL DEFAULT gen_random_uuid() PRIMARY KEY,
    user_id      uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name         text,
    platform     text,
    token_hash   text        NOT NULL,
    last_seen_at timestamptz,
    created_at   timestamptz NOT NULL DEFAULT now()
);

-- media_id = "<source>:<external_id>", e.g. "yt:dQw4w9WgXcQ", "local:01HZ…"
CREATE TABLE artists (
    media_id     text        NOT NULL PRIMARY KEY,
    source_type  text        NOT NULL,
    name         text        NOT NULL,
    available    boolean     NOT NULL DEFAULT true,
    raw_metadata jsonb       NOT NULL DEFAULT '{}',
    created_at   timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE albums (
    media_id           text        NOT NULL PRIMARY KEY,
    source_type        text        NOT NULL,
    title              text        NOT NULL,
    primary_artist_id  text        REFERENCES artists (media_id),
    year               int,
    available          boolean     NOT NULL DEFAULT true,
    raw_metadata       jsonb       NOT NULL DEFAULT '{}',
    created_at         timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE songs (
    media_id          text             NOT NULL PRIMARY KEY,
    source_type       text             NOT NULL,
    title             text             NOT NULL,
    duration_ms       int,
    album_id          text             REFERENCES albums (media_id),
    primary_artist_id text             REFERENCES artists (media_id),
    explicit          boolean          NOT NULL DEFAULT false,
    video_only        boolean          NOT NULL DEFAULT false,
    available         boolean          NOT NULL DEFAULT true,
    loudness_db       double precision,
    last_resolved_at  timestamptz,
    raw_metadata      jsonb            NOT NULL DEFAULT '{}'
);

CREATE TABLE song_artists (
    song_media_id   text NOT NULL REFERENCES songs   (media_id) ON DELETE CASCADE,
    artist_media_id text NOT NULL REFERENCES artists (media_id) ON DELETE CASCADE,
    position        int  NOT NULL DEFAULT 0,
    PRIMARY KEY (song_media_id, artist_media_id)
);

CREATE TABLE playlists (
    id          uuid        NOT NULL DEFAULT gen_random_uuid() PRIMARY KEY,
    user_id     uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    title       text        NOT NULL,
    source_type text        NOT NULL,
    external_id text,
    version     bigint      NOT NULL DEFAULT 1,
    created_at  timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE playlist_items (
    playlist_id        uuid        NOT NULL REFERENCES playlists (id) ON DELETE CASCADE,
    position           int         NOT NULL,
    song_media_id      text        NOT NULL REFERENCES songs   (media_id),
    added_at           timestamptz NOT NULL DEFAULT now(),
    added_by_device_id uuid        REFERENCES devices (id),
    PRIMARY KEY (playlist_id, position)
);

-- +goose StatementEnd

-- +goose Down
-- +goose StatementBegin

DROP TABLE IF EXISTS playlist_items  CASCADE;
DROP TABLE IF EXISTS playlists       CASCADE;
DROP TABLE IF EXISTS song_artists    CASCADE;
DROP TABLE IF EXISTS songs           CASCADE;
DROP TABLE IF EXISTS albums          CASCADE;
DROP TABLE IF EXISTS artists         CASCADE;
DROP TABLE IF EXISTS devices         CASCADE;
DROP TABLE IF EXISTS users           CASCADE;

-- +goose StatementEnd
