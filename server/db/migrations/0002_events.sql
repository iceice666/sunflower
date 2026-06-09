-- +goose Up
-- +goose StatementBegin

CREATE TABLE play_events (
    id             uuid        NOT NULL DEFAULT gen_random_uuid() PRIMARY KEY,
    user_id        uuid        NOT NULL REFERENCES users   (id),
    device_id      uuid        REFERENCES devices (id),
    song_media_id  text        NOT NULL REFERENCES songs   (media_id),
    queue_id       uuid,
    kind           text        NOT NULL,
    occurred_at    timestamptz NOT NULL,
    total_played_ms int,
    reason         text
);

-- For most-played queries
CREATE INDEX idx_play_events_song ON play_events (user_id, song_media_id, occurred_at DESC);
-- For recent + forgotten-favorites queries
CREATE INDEX idx_play_events_recent ON play_events (user_id, occurred_at DESC);

-- Composite PK: one like row per (user, song); idempotency_key ensures deduplication
CREATE TABLE likes (
    user_id        uuid        NOT NULL REFERENCES users (id),
    song_media_id  text        NOT NULL REFERENCES songs (media_id),
    liked_at       timestamptz NOT NULL DEFAULT now(),
    idempotency_key uuid       UNIQUE,
    PRIMARY KEY (user_id, song_media_id)
);

CREATE TABLE recommendation_impressions (
    id         uuid        NOT NULL DEFAULT gen_random_uuid() PRIMARY KEY,
    user_id    uuid        NOT NULL REFERENCES users (id),
    section_id text,
    source     text,
    seed_id    text,
    media_id   text,
    shown_at   timestamptz NOT NULL DEFAULT now(),
    clicked_at timestamptz,
    position   int
);

-- +goose StatementEnd

-- +goose Down
-- +goose StatementBegin

DROP TABLE IF EXISTS recommendation_impressions CASCADE;
DROP TABLE IF EXISTS likes                      CASCADE;
DROP TABLE IF EXISTS play_events                CASCADE;

-- +goose StatementEnd
