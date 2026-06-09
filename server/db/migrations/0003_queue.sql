-- +goose Up
-- +goose StatementBegin

CREATE TABLE queue_sessions (
    id        uuid        NOT NULL DEFAULT gen_random_uuid() PRIMARY KEY,
    user_id   uuid        NOT NULL REFERENCES users   (id),
    device_id uuid        REFERENCES devices (id),
    seed_kind text,
    seed_id   text,
    version   bigint      NOT NULL DEFAULT 1,
    title     text,
    items     jsonb       NOT NULL DEFAULT '[]',
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE queue_items (
    queue_id    uuid NOT NULL REFERENCES queue_sessions (id) ON DELETE CASCADE,
    position    int  NOT NULL,
    media_id    text NOT NULL,
    source_data jsonb NOT NULL DEFAULT '{}',
    PRIMARY KEY (queue_id, position)
);

-- +goose StatementEnd

-- +goose Down
-- +goose StatementBegin

DROP TABLE IF EXISTS queue_items    CASCADE;
DROP TABLE IF EXISTS queue_sessions CASCADE;

-- +goose StatementEnd
