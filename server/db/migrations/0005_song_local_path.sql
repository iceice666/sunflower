-- +goose Up
-- +goose StatementBegin
ALTER TABLE songs ADD COLUMN local_path text;
-- +goose StatementEnd

-- +goose Down
-- +goose StatementBegin
ALTER TABLE songs DROP COLUMN local_path;
-- +goose StatementEnd
