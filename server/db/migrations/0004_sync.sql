-- +goose Up
-- +goose StatementBegin

CREATE TABLE idempotency_log (
    key           uuid        NOT NULL PRIMARY KEY,
    user_id       uuid        REFERENCES users   (id),
    device_id     uuid        REFERENCES devices (id),
    route         text        NOT NULL,
    response_hash text,
    created_at    timestamptz NOT NULL DEFAULT now(),
    expires_at    timestamptz
);

CREATE TABLE rec_cache (
    cache_key    text        NOT NULL PRIMARY KEY,
    user_id      uuid        REFERENCES users (id),
    payload      jsonb       NOT NULL,
    generated_at timestamptz NOT NULL DEFAULT now(),
    expires_at   timestamptz
);

-- Cookie encryption: libsodium crypto_secretbox in the app layer (key from
-- SUNFLOWER_COOKIE_KEY). Table ships in M0; logic is implemented in M-later.
CREATE TABLE encrypted_cookies (
    user_id         uuid        NOT NULL REFERENCES users (id),
    provider        text        NOT NULL,
    ciphertext      bytea       NOT NULL,
    nonce           bytea       NOT NULL,
    refreshed_at    timestamptz,
    expires_at_hint timestamptz,
    PRIMARY KEY (user_id, provider)
);

CREATE TABLE downloaded_tracks (
    device_id       uuid        NOT NULL REFERENCES devices (id),
    song_media_id   text        NOT NULL REFERENCES songs   (media_id),
    local_path      text        NOT NULL,
    bytes           bigint,
    completed_at    timestamptz,
    last_verified_at timestamptz,
    PRIMARY KEY (device_id, song_media_id)
);

-- +goose StatementEnd

-- +goose Down
-- +goose StatementBegin

DROP TABLE IF EXISTS downloaded_tracks  CASCADE;
DROP TABLE IF EXISTS encrypted_cookies  CASCADE;
DROP TABLE IF EXISTS rec_cache          CASCADE;
DROP TABLE IF EXISTS idempotency_log    CASCADE;

-- +goose StatementEnd
