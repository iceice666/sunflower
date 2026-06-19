# Post-V1 — Visual Verification Plan

## Why this exists

M0–M8 are marked complete and the **server** half is honestly verified
(`go build` / `go test` / `gofmt` / `sqlc` all green). The **client** half is
not: every milestone note says the Flutter code was only `dart format`-verified
because no Flutter SDK was available in-env. Nothing has ever been *rendered* or
*run* on a device.

This plan closes that gap: prove each milestone's **visual demo target** actually
renders correctly, and leave behind regression protection so it stays that way.

## Environment reality (measured)

| Thing | State | Implication |
|---|---|---|
| Go toolchain | in `nix develop` shell | server runs locally |
| Flutter SDK | in `nix develop` via `flake.nix` (`flutter` 3.41.9 / Dart 3.11.5) | Phase 0 SDK blocker cleared |
| macOS native assets | Flutter wrapped to use host Xcode `/usr/bin/xcrun` + SDK | fixes `objective_c`/`-isysroot` hook failures |
| Android | `~/Library/Android/sdk`, `adb`+`emulator`, AVD `Pixel_10` | live verify = Android |
| iOS | no `simctl` (CLT only, no full Xcode) | iOS verify **deferred** |
| CI | none (`.github/workflows` absent) | add in Phase 5 |
| Client tests | 1 widget test, 0 goldens; `flutter test` now runs | TBD: fix client compile/test failures before goldens |
| Generated code | `flutter pub run build_runner build` works in-env | commit policy for generated files still TBD |

## Strategy — two layers (hybrid)

- **Layer 1 — Golden tests** (deterministic, mocked providers, fixed data).
  The regression backbone: renders each screen in isolation, runs in CI, no
  server/emulator needed. Catches visual drift on every PR.
- **Layer 2 — Live emulator smoke** (`Pixel_10` + real `sunflowerd` + seeded
  Postgres). Proves the actual end-to-end demo targets with captured
  screenshots. Slower, lower frequency, Android-only.

Layer 1 proves *the widget looks right*; Layer 2 proves *the system delivers it*.

## Screen → milestone → states to capture

| Screen(s) | Milestone | States (visual matrix) |
|---|---|---|
| `onboarding/server_setup` | M1/M2 | empty form, validating, error, success |
| `library/songs` + `player_ui/mini_player` | M2 | empty, populated, playing (mini bar) |
| `player_ui/now_playing` + `queue_panel` | M2/M4 | playing, queue open, lookahead list |
| `home` + `section_widget` + `chip_bar` | M5 | loading, live sections, **cold-start cached**, chip switch |
| `library/playlists` + `playlist_detail` + `download_button` | M5/M6 | list, detail, download CTA states |
| `downloads_ui/downloads_screen` | M6 | in-progress, complete, **airplane-mode playback** |
| `settings/settings_screen` + `sync_status_widget` | M7 | pending **N>0** → drains to **0** |
| `settings/crossfade_setting` | M8 | toggle off/on, duration slider |
| now-playing live update (mini/now-playing) + `/admin` | M8 | two-client live tick, admin JSON |

Baseline per-screen state set: **loading / empty / populated / error / offline**.

## Phases

### Phase 0 — Toolchain & reproducible build
- `flutter` is available in the `flake.nix` devShell. The pinned nixpkgs
  package provides Flutter 3.41.9 and Dart 3.11.5, satisfying
  `sdk: '>=3.5.0 <4.0.0'`.
- macOS Flutter commands are wrapped to use the host Xcode toolchain for native
  asset hooks. Without this, nix's `xcbuild` `xcrun` shim can pass
  `error: unable to find sdk: 'macosx'` as `-isysroot` during `objective_c`
  builds.
- `flutter pub get` succeeds; use `flutter pub run build_runner build` for
  Drift/mockito/json codegen. `dart run build_runner build` cannot resolve
  Flutter SDK packages such as `flutter_test`.
- Remaining TBDs before Phase 1:
  - implement or replace missing `LocalRadio` referenced by
    `core/player/sunflower_audio_handler.dart`;
  - fix `MiniPlayer` resolution in `features/home/home_screen.dart`;
  - fix `ReplayBuffer` test/database lifecycle and retry-count failures;
  - boot `Pixel_10` via `emulator -avd Pixel_10` and capture first render.

### Phase 1 — Golden harness
- Adopt a golden lib (recommend **alchemist** or `golden_toolkit`) for
  deterministic font loading + multi-device sizing (avoids host-font flakiness).
- Provider-override scaffolding: fake repositories + fixtures. Reuse server
  JSON fixtures (`internal/innertube/parser/testdata/`) where shapes match.
- Pin device config: Pixel-class size, dark theme, fixed text scale.
- Establish `--update-goldens` baseline + review workflow.

### Phase 2 — Per-screen goldens
- One golden test per screen × key states from the matrix (~16 widgets).
- Mock at the provider boundary so YT-dependent screens are deterministic.

### Phase 3 — Live emulator smoke
- Seed script / Make target: Postgres up + migrations + a demo music folder
  scanned + a device token minted (+ optional YT cookies loaded).
- `client/integration_test/visual_smoke_test.dart`: drives the app through the
  8 demo targets, captures screenshots to an artifacts dir.
- Checklist doc mapping each screenshot → milestone acceptance criterion.

### Phase 4 — Dynamic / WS checks
- Two clients: confirm now-playing live tick (~1 Hz) propagates.
- `GET /admin` snapshot reflects active device.
- M7 drain: offline mutations → reconnect → "N pending" → 0.

### Phase 5 — CI & report
- `.github/workflows/client-verify.yml`: `flutter test` (goldens) on every PR;
  emulator smoke optional/nightly via `reactivecircus/android-emulator-runner`.
- Write a verification report (milestone-doc style); flip `plans/README.md`
  index to reflect "V1 client visually verified".

## Deliverables
- `client/test/goldens/**` + golden tests
- `client/integration_test/visual_smoke_test.dart`
- seed script (`scripts/seed-demo.sh` or Make target)
- `.github/workflows/client-verify.yml`
- `flake.nix` Flutter addition
- this plan + a results report

## Decision points (defaults in **bold**)
1. Flutter via **nix flake** vs fvm vs system install.
2. Scope: **hybrid** (goldens + live smoke) vs goldens-only.
3. iOS: **deferred** (no simulator) vs provision Xcode.
4. YT cookies present for live YT/home sections? If **not**, Layer 2 verifies
   guest-mode *degraded* states instead of populated feeds.
5. Golden lib: **alchemist/golden_toolkit** vs bare `flutter_test` goldens.

## Risks
- **Golden flakiness** across host vs CI fonts/platform → run in a fixed
  container; use the golden lib's font loading; never diff macOS-host goldens
  against Linux CI without tolerance.
- **Codegen drift** (`build_runner`) → commit generated files or run in CI.
- **YT nondeterminism** → mock at provider boundary for goldens; only live-test
  in Layer 2.
- **Audio is invisible** → verify via player UI state + position ticks, not sound.

## Out of scope
- iOS / desktop / web visual verification (no sims; platforms not scaffolded
  beyond android/ios).
- Pixel-perfect design review (this is correctness, not design QA).
- Anything in `risks.md` "out of scope for v1".
