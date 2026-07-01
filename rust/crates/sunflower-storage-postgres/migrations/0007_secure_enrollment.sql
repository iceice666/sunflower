-- +goose Up
-- +goose StatementBegin

ALTER TABLE users
  ADD COLUMN admin_password_hash text,
  ADD COLUMN admin_password_updated_at timestamptz;

ALTER TABLE devices
  ADD COLUMN token_label text,
  ADD COLUMN revoked_at timestamptz,
  ADD COLUMN revoked_reason text;

CREATE TABLE admin_sessions (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  token_hash text NOT NULL UNIQUE,
  csrf_secret_hash text NOT NULL,
  expires_at timestamptz NOT NULL,
  last_seen_at timestamptz,
  revoked_at timestamptz,
  created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE pairing_codes (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  code_hash text NOT NULL UNIQUE,
  label text,
  expires_at timestamptz NOT NULL,
  used_at timestamptz,
  used_by_device_id uuid REFERENCES devices(id),
  created_by_session_id uuid REFERENCES admin_sessions(id),
  created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE audit_events (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid REFERENCES users(id) ON DELETE SET NULL,
  actor_type text NOT NULL,
  actor_id text,
  event text NOT NULL,
  target_type text,
  target_id text,
  metadata jsonb NOT NULL DEFAULT '{}',
  created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_devices_active ON devices(user_id, last_seen_at DESC)
  WHERE revoked_at IS NULL;
CREATE INDEX idx_pairing_codes_expiry ON pairing_codes(expires_at)
  WHERE used_at IS NULL;
CREATE INDEX idx_admin_sessions_active ON admin_sessions(user_id, expires_at)
  WHERE revoked_at IS NULL;
CREATE INDEX idx_audit_events_recent ON audit_events(created_at DESC);

-- +goose StatementEnd

-- +goose Down
-- +goose StatementBegin

DROP INDEX IF EXISTS idx_audit_events_recent;
DROP INDEX IF EXISTS idx_admin_sessions_active;
DROP INDEX IF EXISTS idx_pairing_codes_expiry;
DROP INDEX IF EXISTS idx_devices_active;

DROP TABLE IF EXISTS audit_events CASCADE;
DROP TABLE IF EXISTS pairing_codes CASCADE;
DROP TABLE IF EXISTS admin_sessions CASCADE;

ALTER TABLE devices
  DROP COLUMN IF EXISTS revoked_reason,
  DROP COLUMN IF EXISTS revoked_at,
  DROP COLUMN IF EXISTS token_label;

ALTER TABLE users
  DROP COLUMN IF EXISTS admin_password_updated_at,
  DROP COLUMN IF EXISTS admin_password_hash;

-- +goose StatementEnd
