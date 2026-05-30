# M8 — WebSocket Now-Playing + Polish

## Demo target

- Play on phone.
- Open admin tab in a desktop browser (or a second client) connected to the
  same server.
- See the currently-playing track and position update live (~1 Hz) without
  reload.
- Optional: a `Pause` button in the admin tab issues a server → client command
  and the phone pauses.

## Scope

- WebSocket hub (`internal/ws`) with subprotocol
  `sunflower.now-playing.v1`.
- Client position-tick emission from the player handler.
- Server → client command channel for remote control.
- Minimal admin endpoint `/admin` returning a JSON dashboard
  (now-playing + buffer status + cookie status).
- Optional: crossfade via a secondary `just_audio` player, behind a setting.

## Files to create

```
server/internal/ws/
  hub.go                  # client registry, broadcast, subprotocol negotiation
  conn.go                 # per-connection state, ping/pong heartbeat
  protocol.go             # tick / transition / state / command JSON shapes
server/internal/api/
  handlers_ws.go          # GET /api/v1/ws/now-playing upgrade
  handlers_admin.go       # GET /admin (JSON only, no UI in v1)

client/lib/core/ws/
  now_playing_socket.dart # persistent WebSocket, reconnect with backoff
  tick_emitter.dart       # subscribes to AudioHandler position stream → emits
  command_handler.dart    # incoming server commands → AudioHandler
client/lib/core/player/
  crossfade_player.dart   # OPTIONAL: secondary AudioPlayer, swap on transition
client/lib/features/settings/
  crossfade_setting.dart  # toggle + duration slider
```

## Acceptance criteria

- WebSocket reconnects with backoff (5 s → 30 s → 5 min cap) after server
  bounce; no missed ticks during a quick reconnect.
- Pause command from admin reaches client within 500 ms over LAN.
- Tick rate is ~1 Hz during playback; goes silent when paused (no noisy
  no-op ticks).
- `/admin` returns currently-playing track per active device.
- Crossfade (if enabled): transition between two tracks is audibly smooth
  with no gap; queue position and shuffle stay consistent during the swap.

## Dependencies on prior milestones

- M2 player baseline.
- M4 queue / `/next` (the WS reports what is currently playing).
- M7 sync (for sane backoff / reconnect semantics).

## Verification

- Unit test: protocol JSON encode/decode round-trip.
- Integration test with a `gorilla/websocket` client in Go: connect, send
  ticks, receive a pause command.
- Manual: two clients, observe live updates.
- Crossfade test: enable, play a queue, listen — no glitches.

## Out of M8 scope

- Web admin UI beyond raw JSON (would be a v2 feature).
- Audio offload / silence skipping as user-facing features.
- Lyrics / EQ / Discord scrobble — all explicitly out of v1 per
  [`../risks.md`](../risks.md).

## After M8

Project is feature-complete for v1. Suggested follow-ups (not in this plan):

- Desktop / iOS / web platform polish.
- Lyrics provider integration.
- LastFM / Discord scrobble.
- Multi-user support (schema is ready; auth/recs scoping needs work).
- v2 recommendations: collaborative filtering over `play_events` once enough
  data accumulates.
