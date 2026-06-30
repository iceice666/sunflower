# M10 — Admin Dashboard

## Demo target

- Open `https://<server>/admin/` in a desktop browser.
- Log in with the M9 owner password.
- See server health, DB status, library counts, YouTube cookie health, active
  devices, and now-playing state.
- Create a pairing code/QR, pair a client, then revoke that device.
- Trigger a library scan and watch job progress.
- Upload/update YouTube cookies and see health change.
- Send play/pause/skip commands to an active device from the dashboard.

## Product goal

M8 exposed `/api/v1/admin` as raw JSON. M10 turns admin operations into a
usable browser surface for self-hosting: setup, devices, pairing, scans,
cookies, now-playing, and basic audit visibility.

The dashboard is intentionally server-rendered. A Go `html/template` UI keeps
deployment simple, avoids a second frontend build chain, and is enough for the
operational workflows.

## Scope

### Dashboard foundation

- `GET /admin/` overview page.
- `GET /admin/login` login page.
- `POST /admin/login` form login backed by M9 admin sessions.
- `POST /admin/logout`.
- shared layout, navigation, flash messages, and error pages.
- embedded CSS/JS/static assets via Go `embed`.
- CSRF token on every mutating form.
- admin JSON endpoints use the same session and require `X-CSRF-Token`.

### Overview page

Shows:

- server version and uptime
- database connectivity
- library counts: songs, albums, artists, playlists
- latest library scan job and status
- YouTube cookie health
- active/recent devices
- now-playing summary
- pending warnings: no cookies, no recent scan, expiring pairing codes,
  repeated auth failures

### Devices and pairing

- list devices with name, platform, created time, last seen, revoked state
- create pairing code with label and TTL
- display pairing code and QR once
- revoke a device with reason
- show device audit history
- never display existing bearer tokens

### Library operations

- configure scan roots if the server supports runtime roots; otherwise show
  configured roots read-only
- trigger a scan
- show scan job progress and recent scan errors
- link to job detail JSON for troubleshooting

### YouTube cookies

- show cookie status, last refresh, last probe result, and failure reason
- upload/update cookies
- trigger health probe
- clear cookies
- never echo raw cookie contents after submit

### Now playing and remote control

- list connected devices and latest now-playing state
- update position/state at about 1 Hz by polling or SSE
- send play, pause, skip next, and skip previous commands
- surface delivery count / offline-device errors

### Audit log

- list recent audit events
- filters: event type, actor type, target type, time window
- show metadata with secrets redacted

## Route map

### HTML routes

```
GET  /admin/
GET  /admin/login
POST /admin/login
POST /admin/logout

GET  /admin/devices
POST /admin/devices/{id}/revoke

GET  /admin/pairing/new
POST /admin/pairing

GET  /admin/library
POST /admin/library/scan

GET  /admin/cookies/youtube
POST /admin/cookies/youtube
POST /admin/cookies/youtube/probe
POST /admin/cookies/youtube/clear

GET  /admin/now-playing
POST /admin/now-playing/command

GET  /admin/audit
```

### JSON routes

```
GET  /api/v1/admin/status
GET  /api/v1/admin/devices
POST /api/v1/admin/devices/{id}/revoke
POST /api/v1/admin/pairing-codes
GET  /api/v1/admin/library/status
POST /api/v1/admin/library/scan
GET  /api/v1/admin/cookies/youtube/status
POST /api/v1/admin/cookies/youtube
POST /api/v1/admin/cookies/youtube/probe
POST /api/v1/admin/cookies/youtube/clear
GET  /api/v1/admin/now-playing
POST /api/v1/admin/now-playing/command
GET  /api/v1/admin/audit
```

Compatibility:

- Keep M8 `GET /api/v1/admin` as an alias for `/api/v1/admin/status`.
- New browser UI lives at `/admin/`, outside `/api/v1`.

## Files to create or change

```
server/internal/adminui/
  embed.go
  router.go
  viewmodel.go
  csrf.go
  flash.go
  templates/
    layout.html
    login.html
    overview.html
    devices.html
    pairing_new.html
    library.html
    cookies_youtube.html
    now_playing.html
    audit.html
    error.html
  static/
    admin.css
    admin.js

server/internal/api/
  handlers_admin_status.go       # JSON status payload, M8 alias
  handlers_admin_devices.go      # JSON device list/revoke
  handlers_admin_library.go      # JSON scan/status
  handlers_admin_cookies.go      # JSON cookie status/upload/probe/clear
  handlers_admin_nowplaying.go   # JSON now-playing + command
  handlers_admin_audit.go        # JSON audit listing

server/internal/db/query/
  admin.sql                      # dashboard list/count/status queries

server/cmd/sunflowerd/
  main.go                        # mount /admin and admin JSON routes
```

## UI requirements

- Dense, operational UI; no marketing/landing page.
- Works on desktop and tablet; mobile layout should remain usable for emergency
  device revocation or pairing.
- Clear status colors:
  - healthy
  - warning
  - error
  - unknown
- Every destructive action has a confirmation step or explicit reason field.
- Pairing code page makes the code easy to read and copy, and includes a QR
  payload.
- No page displays raw bearer tokens, raw session tokens, admin password hashes,
  or raw YouTube cookie content.

## Acceptance criteria

- Unauthenticated browser request to `/admin/` redirects to `/admin/login`.
- Unauthenticated JSON request to `/api/v1/admin/status` returns 401.
- Login with a valid M9 owner password creates an admin session cookie.
- All mutating admin routes reject missing or invalid CSRF tokens.
- Overview page renders with:
  - server uptime
  - DB status
  - library counts
  - cookie status
  - active/recent devices
  - now-playing summary
- Pairing page:
  - creates a code
  - displays it once
  - shows expiry
  - QR payload pairs a client successfully
- Device revoke:
  - marks `revoked_at`
  - writes audit event
  - immediately blocks that device's HTTP and WebSocket auth
- Library page:
  - starts a scan through existing job registry
  - shows progress without reloading the server
- Cookie page:
  - uploads cookies through existing encrypted cookie store
  - shows health probe result
  - redacts cookie content in logs and UI
- Now-playing page:
  - updates active devices without full page reload
  - sends pause/play/skip commands through existing M8 hub
- Audit page:
  - lists recent security and admin operations
  - redacts sensitive metadata

## Security requirements

- Admin session cookie:
  - HttpOnly
  - SameSite=Lax
  - Secure when request is HTTPS or trusted proxy marks HTTPS
  - scoped to `/admin`
- CSRF:
  - required for all POST/PATCH/DELETE admin HTML and JSON routes
  - token is per-session and not stored in local storage
- Content security:
  - no inline third-party scripts
  - static assets are embedded locally
  - cookie upload body has a size limit
- Logging:
  - redact passwords, pairing codes, session tokens, device tokens, and cookie
    contents
  - audit operation outcome, not secret values

## Dependencies on prior milestones

- M8 now-playing hub and command path.
- M9 admin sessions, CSRF, pairing codes, device revocation, and audit events.
- M1/M6/M7 job registry, cookie store, downloads registry, and idempotency
  behavior.

## Verification

- Server unit tests:
  - template rendering for every page
  - CSRF accept/reject
  - admin middleware redirect vs JSON 401 behavior
  - secret redaction helpers
- Server integration tests:
  - login -> overview -> create pairing code -> register device
  - login -> revoke device -> device token rejected
  - login -> trigger scan -> job status visible
  - login -> upload cookie -> status visible
  - login -> send now-playing command -> hub delivery count
- Browser smoke tests:
  - Playwright desktop viewport: login, overview, devices, pairing, library,
    cookies, now-playing, audit
  - Playwright mobile viewport: login, create pairing code, revoke device
  - screenshot artifacts committed or uploaded in CI
- Manual:
  - pair a real phone from dashboard QR
  - verify no token/cookie appears in browser source, logs, or audit metadata

## Out of M10 scope

- React/Vue/Svelte SPA.
- Multi-user role management.
- Public sharing links.
- Editing playlists/library metadata from dashboard.
- Desktop bundled localhost server / "This computer" mode. That should build
  on M9/M10 in a later milestone.
- Mobile embedded server.
- Advanced observability stack (Prometheus/Grafana). M10 only exposes
  dashboard status and simple JSON.

## Implementation status

Planned. No server/client code has been changed for M10 yet.
