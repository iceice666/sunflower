-- +goose Up
-- +goose StatementBegin

CREATE TABLE cookie_health (
    provider     text        NOT NULL PRIMARY KEY,
    status       text        NOT NULL DEFAULT 'unknown',
    checked_at   timestamptz,
    detail       text
);

-- +goose StatementEnd

-- +goose Down
-- +goose StatementBegin
DROP TABLE IF EXISTS cookie_health;
-- +goose StatementEnd
