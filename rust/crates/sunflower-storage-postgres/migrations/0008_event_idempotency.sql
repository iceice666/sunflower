-- +goose Up
-- +goose StatementBegin

-- Per-event idempotency for batched playback feedback. The request-level
-- idempotency_log dedupes one HTTP replay; this table dedupes the event_id
-- carried inside /api/v1/events when a client re-batches the same event.
CREATE TABLE IF NOT EXISTS rust_ingested_events (
    user_id    uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    event_id   text        NOT NULL,
    device_id  uuid        REFERENCES devices (id) ON DELETE SET NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, event_id)
);

-- +goose StatementEnd

-- +goose Down
-- +goose StatementBegin

DROP TABLE IF EXISTS rust_ingested_events CASCADE;

-- +goose StatementEnd
