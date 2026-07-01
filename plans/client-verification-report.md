# Sunflower — V1 Client Visual Verification Report

> **Status: verified locally.** Both layers were executed on 2026-06-19 against
> Flutter 3.41.9 / Dart 3.11.5 (nix devShell): 22 golden tests pass on the
> committed baselines, and the live `flutter drive` smoke ran on the `Pixel_10`
> AVD against a real `sunflowerd` + seeded Postgres, capturing all 9 screenshots
> + the `/admin` JSON snapshot to `client/build/smoke-artifacts/`. CI wires the
> same two layers (goldens on every PR, smoke nightly). This document maps every
> captured artifact to its milestone acceptance criterion.

---

## Layer 1 — Golden tests (deterministic, mocked providers)

All goldens live under `client/test/goldens/goldens/snapshots/` as `.png` files.
Baselines are committed to the repository. The PR workflow (`golden-tests` job in
`.github/workflows/client-verify.yml`) fails on pixel diff, locking in visual
regressions on every merge.

### Checklist

| Golden file (relative to `snapshots/`) | Screen | Milestone | Acceptance criterion verified |
|---|---|---|---|
| `onboarding/server_setup.png` | `server_setup_screen.dart` | M1 | Empty server-URL form renders; submit CTA visible |
| `library/songs_screen_populated.png` | `songs_screen.dart` | M2 | Song list renders title + artist rows from mocked API |
| `library/songs_screen_empty.png` | `songs_screen.dart` | M2 | Empty-library zero-state copy visible |
| `library/songs_screen_error.png` | `songs_screen.dart` | M2 | Network error banner/copy visible |
| `library/playlists_populated.png` | `playlists_screen.dart` | M5/M6 | Playlist list renders name + track-count rows |
| `library/playlists_empty.png` | `playlists_screen.dart` | M5/M6 | Empty-playlist state visible |
| `library/playlist_detail_populated.png` | `playlist_detail_screen.dart` | M6 | Detail view shows track list + download CTA |
| `library/playlist_detail_empty.png` | `playlist_detail_screen.dart` | M6 | Empty playlist detail state visible |
| `player/mini_player_idle.png` | `mini_player.dart` | M2 | Mini-player bar collapsed / hidden when idle |
| `player/mini_player_playing.png` | `mini_player.dart` | M2 | Mini-player bar shows track title + play/pause CTA |
| `player/now_playing.png` | `now_playing_screen.dart` | M2/M4 | Full-screen player shows art, title, controls |
| `player/queue_panel_empty.png` | `queue_panel.dart` | M4 | Queue panel empty state visible |
| `player/queue_panel_populated.png` | `queue_panel.dart` | M4 | Queue panel open; lookahead list items rendered |
| `home/home_sections.png` | `home_screen.dart` + `section_widget.dart` | M5 | Home feed populated with section rows |
| `home/home_stale.png` | `home_screen.dart` | M5 | Cold-start cached/offline banner visible |
| `home/home_error.png` | `home_screen.dart` | M5 | Error state with retry visible |
| `downloads/downloads_empty.png` | `downloads_screen.dart` | M6 | Empty downloads state visible |
| `downloads/downloads_screen.png` | `downloads_screen.dart` | M6 | In-progress, failed, and completed download rows with status badges |
| `settings/settings_screen.png` | `settings_screen.dart` | M7/M8 | Settings rows visible; navigation targets present |
| `settings/sync_status_pending.png` | `sync_status_widget.dart` | M7 | Pending-mutation count and overflow-drop state displayed |
| `settings/crossfade_disabled.png` | `crossfade_setting.dart` | M8 | Crossfade disabled toggle state rendered |
| `settings/crossfade_enabled.png` | `crossfade_setting.dart` | M8 | Crossfade enabled toggle + duration slider rendered |

**Total goldens: 22** across 8 milestones (M1–M8), all screens in
`plans/post-v1-visual-verification.md §Screen → milestone → states to capture`.

---

## Layer 2 — Android emulator smoke (nightly)

**Target:** `client/integration_test/visual_smoke_test.dart`  
**Runner:** `reactivecircus/android-emulator-runner@v2`, API 33, `pixel_6` profile, 1080×2400 (Pixel 10-class)  
**Schedule:** nightly 02:00 UTC via `.github/workflows/client-verify.yml` `android-smoke` job  
**Capture path:** ADB pull from `/sdcard/Android/data/com.iceice666.sunflower/files/sunflower-smoke/` → `client/build/smoke-artifacts/`  
**Output:** uploaded as `smoke-screenshots-<run_id>` artifact, retained 30 days

### Screenshot → milestone criterion map

Screenshot filenames are relative to `client/build/smoke-artifacts/` (pulled by ADB post-run).
Server seeded via `just seed-demo` (`scripts/seed-demo.sh`).

| Screenshot file | Demo flow step | Milestone | Acceptance criterion |
|---|---|---|---|
| `t01_onboarding_setup.png` | Enter server URL, tap Connect | M1/M2 | Server-setup form renders; device registers on submit |
| `t02_library_songs.png` | Library tab loaded after scan | M2 | Scanned songs visible in list; title + artist rows present |
| `t03_mini_player.png` | Tap a track; mini-player visible | M2 | Mini-player bar persists across tab navigation during playback |
| `t04_now_playing.png` | Open full now-playing screen | M2/M4 | Full player shows art, title, controls; queue items rendered from `/next` lookahead |
| `t05_home_feed.png` | Home tab with server data | M5 | Recommendation sections rendered; cold-start cached render visible |
| `t06_playlists.png` | Playlists tab | M5/M6 | Playlist list populated; download CTA visible on playlist items |
| `t07_downloads.png` | Download a playlist; downloads tab | M6 | Download progress + completed rows shown; airplane-mode playback verified in test body |
| `t08_settings_sync.png` | Settings screen; offline mutation + reconnect | M7/M8 | Sync-status widget shows pending count draining to 0; all settings rows present |
| `t09_ws_tick.png` | Second client connects; observe now-playing push | M8 | Live now-playing update propagates (~1 Hz tick) to first client view |
| `t10_admin_snapshot.json` | HTTP `GET /api/v1/admin` response | M8 | `/admin` JSON reflects active device and current track |

**Total smoke artifacts: 9 screenshots + 1 admin JSON snapshot**, all 8 milestone demo targets covered (M1–M8).

> **Live smoke prerequisite:** a running `sunflowerd` with `just seed-demo` applied (which
> mints a device token into `.seed-env`). The nightly CI job provisions Postgres,
> boots Rust `sunflowerd` (which applies embedded migrations), runs `just seed-demo`, then
> `flutter drive` against the AVD — the `.seed-env` token is passed via
> `--dart-define=SUNFLOWER_DEMO_TOKEN`, enabling the WS-tick and `/admin` assertions.
> Screenshots are streamed host-side by `test_driver/integration_test.dart`.

---

## Coverage gaps & known deferred items

| Item | Reason deferred |
|---|---|
| iOS visual verification | No `simctl` available; Xcode-full not installed |
| Web / desktop | Platforms not scaffolded |
| YT-populated home feed | Requires valid YT cookies; guest-mode degraded state covered instead |
| Two-client WS live tick (emulator) | Requires two concurrent devices in CI; `t09_ws_tick.png` captured from single-device approximation |
| Pixel-perfect design review | Out of scope (correctness only) |

---

## Files changed by this verification phase

| File | Author | Change |
|---|---|---|
| `.github/workflows/client-verify.yml` | CIReport | New — PR golden job (`flutter test test/goldens/`) + nightly Android smoke job (`android-emulator-runner@v2`) |
| `plans/client-verification-report.md` | CIReport | New — this report |
| `plans/README.md` | CIReport | Updated — `visually verified` row in milestone index + "V1 client visual verification" section |
| `client/pubspec.yaml` | GoldenHarness | Updated — added `golden_toolkit: ^0.15.0` and `integration_test: sdk: flutter` to dev_dependencies |
| `client/test/goldens/helpers/golden_harness.dart` | GoldenHarness | New — harness entry point (font loading, device config, provider scaffolding) |
| `client/test/goldens/fakes/fixtures.dart` | GoldenHarness | New — shared fixture data |
| `client/test/goldens/fakes/fakes.dart` | GoldenScreens | New — provider-boundary fakes for API, audio, buffered sync |
| `client/test/goldens/goldens/snapshots/**` (22 PNGs) | GoldenScreens | New — committed golden baselines; update with `flutter test --update-goldens test/goldens/` from `client/` |
| `client/test/goldens/*.dart` (screen tests) | GoldenScreens | New — one test file per screen × key states |
| `client/integration_test/visual_smoke_test.dart` | LiveSmoke | New — Android smoke driver; captures to app-specific external storage |
| `scripts/seed-demo.sh` | LiveSmoke | New — seeds Postgres + demo library + device token for smoke run |
| `justfile` | LiveSmoke | Updated — `seed-demo` and `smoke-android` recipes |
| `client/android/**` | Main | Updated — Gradle plugin migration, SDK/NDK bump, launcher icon fix for Flutter 3.41 Android builds |
