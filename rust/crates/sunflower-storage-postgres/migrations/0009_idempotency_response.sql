-- +goose Up
-- +goose StatementBegin

ALTER TABLE idempotency_log
    ADD COLUMN IF NOT EXISTS response_status integer,
    ADD COLUMN IF NOT EXISTS response_body bytea,
    ADD COLUMN IF NOT EXISTS response_content_type text;

-- +goose StatementEnd

-- +goose Down
-- +goose StatementBegin

ALTER TABLE idempotency_log
    DROP COLUMN IF EXISTS response_content_type,
    DROP COLUMN IF EXISTS response_body,
    DROP COLUMN IF EXISTS response_status;

-- +goose StatementEnd
