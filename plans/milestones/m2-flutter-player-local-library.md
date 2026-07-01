# M2 — Flutter Player Against Local Library Only

> **Archive note (2026-07-01):** This milestone is retained as historical
> build and acceptance context from the original Go `server/` implementation.
> The canonical implementation is now Rust under `rust/`; use
> [`../README.md`](../README.md) and [`../architecture.md`](../architecture.md)
> for current crate layout, migrations, assets, and verification commands.

## Demo target

On Android: install Flutter app, paste server URL, register device on first
launch, see a list of songs from the server library, tap a song, hear it play
through `just_audio`, see a lock-screen notification with prev/next controls.

**No recommendations, no queue, no sync, no offline.** This milestone exists
purely to prove the audio pipeline end-to-end.

## Scope

- Flutter project bootstrap (`client/`).
- First-launch device registration flow.
- Library browsing UI (songs list, simple search by title).
- Player + `audio_service` integration.
- OS media session with notification controls.

## Files to create

```
client/
  pubspec.yaml                 # just_audio, audio_service, dio, drift,
                               # flutter_riverpod (or provider), uuid
  lib/main.dart
  lib/app.dart
  lib/core/
    api/sunflower_api.dart     # dio client; only GET /songs + register-device
    auth/token_store.dart      # flutter_secure_storage or shared_prefs (token)
    auth/register_device.dart
    player/sunflower_audio_handler.dart  # BaseAudioHandler subclass
    player/player_bootstrap.dart
  lib/features/
    onboarding/server_setup_screen.dart  # paste server URL, register device
    library/songs_screen.dart            # list view, tap to play
    player_ui/mini_player.dart           # bottom mini-player
    player_ui/now_playing_screen.dart    # full-screen now-playing
android/app/build.gradle       # audio_service Android config
ios/Runner/Info.plist          # background-audio capability
```

## Acceptance criteria

- Cold-start with no token → server setup screen → register-device → token
  stored securely → song list loads.
- Tap song → audio plays within 2 s on a warm cache.
- Lock screen shows track, art, prev/next, play/pause.
- Bluetooth headset play/pause works.
- App killed and re-opened → previous track and position are remembered
  (in-memory + simple persisted last-played in `shared_preferences` for M2;
  proper queue persistence comes in M4).

## Dependencies on prior milestones

- M0, M1 — server running with at least one device registered and a populated
  library.

## Platform priority

Android first. iOS, desktop, web are M9+. Capability matrix lives in
`core/player/capabilities.dart` from day 1 so platform-specific code paths are
explicit, not implicit.

## Verification

- Widget test: songs list renders from a mock API.
- Audio handler smoke test against a local MP3 fixture (`just_audio` supports
  asset playback in tests).
- Manual: golden path is "open app → see list → tap → hear audio → lock screen
  → press pause → audio stops".

## Out of M2 scope

- Albums/artists screens (M5).
- Queue UI, lookahead, recommendations.
- Offline downloads (M6).
- Write-replay (M7).
- WebSocket now-playing (M8).
