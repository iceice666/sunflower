# M9 — Secure Enrollment

## Demo target

- Start a fresh server with no owner configured. The server exposes
  `/api/v1/setup/status` and refuses normal device registration.
- Complete owner setup with a setup token and password.
- Log into the admin API, create a one-time pairing code, then pair a Flutter
  client with `Server URL + Pairing Code`.
- Try to register a second device without a valid pairing code; it fails.
- Revoke the paired device from admin; the client receives `401 device_revoked`,
  clears credentials, and returns to the pairing screen.

## Problem statement

M0-M8 intentionally used a simple single-user device-token model. That proved
the music system, but it leaves enrollment open: any peer that can reach the
server can call `/api/v1/auth/register-device` and get a long-lived bearer
token. M9 closes that gap without changing the core player/recommendation
architecture.

The target model is:

- One owner/admin account per server.
- Browser admin sessions are cookie-based and separate from device tokens.
- Devices can register only with an admin-generated, single-use pairing code.
- Device tokens remain long-lived opaque bearer tokens, but they can be
  revoked.
- Security-sensitive actions are rate-limited and audited.

## Scope

### Server

- First-run owner setup:
  - `GET /api/v1/setup/status`
  - `POST /api/v1/setup/owner`
  - setup token from `SUNFLOWER_SETUP_TOKEN` or a first-run console token
  - setup permanently disabled after the owner password is set
- Admin authentication:
  - `POST /api/v1/admin/auth/login`
  - `POST /api/v1/admin/auth/logout`
  - `GET /api/v1/admin/me`
  - HttpOnly admin session cookie
  - CSRF token for admin mutations
- Pairing:
  - `POST /api/v1/admin/pairing-codes`
  - one-time, short-lived pairing codes
  - QR/pairing URL payload returned for M10 UI use
  - `POST /api/v1/auth/register-device` requires a valid pairing code
- Device lifecycle:
  - add device labels
  - add `revoked_at` / `revoked_reason`
  - auth middleware rejects revoked devices for HTTP and WebSocket requests
  - admin can revoke device tokens
- Security controls:
  - rate limit setup, admin login, and device pairing attempts
  - audit events for setup, login, pairing, device revoke, cookie update, and
    library scan initiation
  - do not log raw admin passwords, raw pairing codes, raw device tokens, or
    YouTube cookie contents

### Flutter client

- Replace "Connect & Register" with a pairing-first onboarding flow:
  - Server URL
  - Pairing code
  - Test connection
  - Pair device
- Store the returned device token and device id as today.
- Handle auth failures:
  - `missing_token` / `invalid_token` / `device_revoked` clears credentials
    and returns to onboarding
  - `pairing_required` and `invalid_pairing_code` show specific errors
  - server not reachable remains distinct from auth failure
- Keep offline downloads behavior intact for already-paired devices.

## API contract

### Public setup status

```
GET /api/v1/setup/status
→ 200 {
  "configured": true,
  "pairing_required": true,
  "server_version": "0.3.0",
  "server_capabilities": [
    "auth.pairing.v1",
    "admin.sessions.v1",
    "device.revoke.v1"
  ]
}
```

No authentication. No secrets. Safe for clients to call on every first-launch
or re-pair attempt.

### Owner setup

```
POST /api/v1/setup/owner
{
  "setup_token": "printed-or-env-token",
  "display_name": "Owner",
  "password": "..."
}
→ 200 {"ok": true}
```

Failure cases:

- `403 setup_disabled` when an owner password already exists.
- `401 invalid_setup_token`.
- `429 rate_limited`.
- `400 weak_password` when password policy fails.

Minimum password policy: 12 characters, not equal to the setup token, and not
empty after trimming. Do not implement external breach-list checks in M9.

### Admin login

```
POST /api/v1/admin/auth/login
{ "password": "..." }
→ 200
Set-Cookie: sf_admin=<session>; HttpOnly; SameSite=Lax; Path=/admin
{
  "csrf_token": "...",
  "expires_at": "2026-07-14T12:00:00Z"
}
```

Sessions expire after 14 days by default. Every successful authenticated admin
request updates `last_seen_at`.

### Pairing code creation

```
POST /api/v1/admin/pairing-codes
X-CSRF-Token: ...
{
  "label": "Pixel 8",
  "ttl_seconds": 600
}
→ 200 {
  "pairing_code": "7K4D-91QF",
  "pairing_url": "sunflower://pair?server=https%3A%2F%2Fmusic.example.com&code=7K4D-91QF",
  "expires_at": "2026-06-30T10:10:00Z"
}
```

Codes:

- default TTL: 10 minutes
- max TTL: 1 hour
- single-use
- generated with at least 40 bits of entropy
- displayed only once
- stored only as a server-secret-derived verifier

### Device registration

```
POST /api/v1/auth/register-device
{
  "device_name": "Pixel 8",
  "platform": "android",
  "client_version": "0.3.0",
  "pairing_code": "7K4D-91QF"
}
→ 200 {
  "device_id": "...",
  "token": "sf_dev_...",
  "server_capabilities": [
    "auth.pairing.v1",
    "library.v1",
    "recs.v1",
    "stream.proxy",
    "ws.now_playing"
  ]
}
```

Failure cases:

- `403 pairing_required` when no code is supplied.
- `401 invalid_pairing_code` for unknown, expired, or already-used codes.
- `429 rate_limited` for repeated failures.

### Device revocation

```
POST /api/v1/admin/devices/{device_id}/revoke
X-CSRF-Token: ...
{ "reason": "Lost phone" }
→ 200 {"ok": true}
```

Revocation takes effect immediately for new HTTP requests and WebSocket
connections. Existing WebSocket connections close on the next auth check or
server-side revoke broadcast.

## Database changes

Create migration `0007_secure_enrollment.sql`.

```sql
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
```

## Files to create or change

```
server/db/migrations/
  0007_secure_enrollment.sql
server/internal/db/query/
  auth.sql                    # extend user/device/session/pairing queries
  admin.sql                   # admin dashboard support queries if separated
server/internal/auth/
  password.go                 # PHC argon2id password hashing/verify
  admin_session.go            # session mint/verify/revoke
  pairing.go                  # code generation, hashing, consume-once
  rate_limit.go               # setup/login/pairing limiter
  audit.go                    # audit event writer
server/internal/api/
  handlers_setup.go           # setup status + owner setup
  handlers_admin_auth.go      # login/logout/me
  handlers_pairing.go         # pairing code create/list/expire
  handlers_devices_admin.go   # admin device list/revoke
server/cmd/sunflowerd/
  main.go                     # first-run setup-token banner / env config

client/lib/features/onboarding/
  server_setup_screen.dart    # replace open register UI with pairing UI
client/lib/core/auth/
  register_device.dart        # include pairing_code
  auth_failure.dart           # classify 401/403 auth failures
client/lib/core/api/
  sunflower_api.dart          # central invalid-token handling hook
```

## Acceptance criteria

- Fresh server with no owner:
  - `/api/v1/setup/status` returns `configured=false`.
  - `/api/v1/auth/register-device` without a pairing code fails.
  - owner setup succeeds only with the setup token.
- Configured server:
  - `/api/v1/setup/owner` always returns `setup_disabled`.
  - admin login creates an HttpOnly cookie session and CSRF token.
  - bad admin login attempts are rate-limited and audited.
- Pairing:
  - admin can create a pairing code.
  - device registration succeeds exactly once with that code.
  - reuse, expired code, missing code, and random code all fail.
  - raw pairing code is not persisted or logged.
- Device auth:
  - normal API calls still accept valid `Authorization: Bearer <device_token>`.
  - revoked devices get `401 device_revoked`.
  - WebSocket auth also rejects revoked devices.
- Client:
  - first launch asks for server URL and pairing code.
  - successful pairing enters the existing app shell.
  - invalid/revoked token clears credentials and returns to onboarding.
  - offline downloads and write-replay buffers are not deleted during re-pair
    unless the user explicitly chooses "forget local data".

## Dependencies on prior milestones

- M1 auth/device table and bearer-token middleware.
- M7 idempotency and mutation plumbing for admin-triggered scan/cookie actions.
- M8 WebSocket auth fallback, because revocation must also apply to
  `?token=` WebSocket connections.

## Verification

- Server unit tests:
  - password hash/verify, wrong-password rejection
  - setup disabled after owner exists
  - pairing code consume-once and expiry
  - revoked device rejected by middleware
  - rate-limit windows
- Server integration tests:
  - fresh setup -> login -> create pairing code -> register device -> API call
  - register without code fails
  - revoke device -> API and WebSocket fail
  - audit rows are written for setup/login/pair/revoke
- Client tests:
  - onboarding validation for URL and pairing code
  - maps `pairing_required`, `invalid_pairing_code`, `device_revoked` to
    distinct UI states
  - credential clear/invalidation reroutes to onboarding
- Manual:
  - pair one phone from a desktop admin session
  - revoke it and confirm the app returns to pairing

## Rollout and compatibility

- M9 is a breaking auth change for first-time registration.
- Existing M0-M8 installations should get a migration path:
  - existing owner row remains
  - existing devices remain valid until revoked
  - admin owner password is unset after migration, so the first admin setup is
    still required
  - once admin setup is complete, new devices require pairing
- Document a temporary emergency flag only for development:
  - `SUNFLOWER_DEV_OPEN_REGISTRATION=1`
  - ignored unless `SUNFLOWER_ENV=development`
  - logs a loud warning on startup

## Out of M9 scope

- Full admin dashboard UI. M9 provides admin auth and JSON endpoints only;
  M10 builds the web dashboard.
- Multi-user / household accounts.
- OAuth, passkeys, SSO, email reset, or external identity providers.
- Mobile embedded local server.
- Rotating device tokens or refresh tokens.

## Implementation status

Complete (2026-06-30). Implemented owner setup, admin sessions, CSRF,
one-time pairing codes, device revocation, rate limiting, audit events, and
pairing-first Flutter onboarding. Verified with `go test ./...` in `server/`
and `flutter analyze` in `client/`.
