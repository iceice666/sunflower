// Golden tests — player UI: MiniPlayer, NowPlayingScreen, QueuePanel.
//
// Baseline: flutter test --update-goldens test/goldens/player_golden_test.dart
// Compare:  flutter test test/goldens/player_golden_test.dart

import 'package:audio_service/audio_service.dart';

import 'package:sunflower/core/player/player_bootstrap.dart';
import 'package:sunflower/features/player_ui/mini_player.dart';
import 'package:sunflower/features/player_ui/now_playing_screen.dart';
import 'package:sunflower/features/player_ui/queue_panel.dart';

import 'helpers/golden_harness.dart';

void main() {
  // ══════════════════════════════════════════════════════════════════════════
  // MiniPlayer
  // ══════════════════════════════════════════════════════════════════════════

  // ── 1. mini_player_idle ───────────────────────────────────────────────────
  // Base overrides have currentMediaItemProvider → Stream.value(null).
  // MiniPlayer returns SizedBox.shrink() when there is no current track.
  testGoldenWidget(
    'mini player — idle (no media)',
    'player/mini_player_idle',
    const MiniPlayer(),
  );

  // ── 2. mini_player_playing ────────────────────────────────────────────────
  // Override both stream providers with a real MediaItem and playing state.
  // These override the base null/empty values; last-wins in the merged list.
  testGoldenWidget(
    'mini player — playing',
    'player/mini_player_playing',
    const MiniPlayer(),
    overrides: [
      currentMediaItemProvider.overrideWith(
        (ref) => Stream.value(Fixtures.mediaItem),
      ),
      playbackStateProvider.overrideWith(
        (ref) => Stream.value(Fixtures.playbackStatePlaying),
      ),
    ],
  );

  // ══════════════════════════════════════════════════════════════════════════
  // NowPlayingScreen
  // ══════════════════════════════════════════════════════════════════════════

  // ── 3. now_playing ────────────────────────────────────────────────────────
  // Full now-playing screen: title, seek bar at 42 s / 90 s, play_arrow icon.
  // Uses playbackStatePaused so PlaybackState.position == updatePosition
  // (42 s, fixed). The playing state uses DateTime.now() for position, making
  // the seek bar slider non-deterministic across golden runs.
  testGoldenWidget(
    'now playing screen — paused',
    'player/now_playing',
    const NowPlayingScreen(),
    overrides: [
      currentMediaItemProvider.overrideWith(
        (ref) => Stream.value(Fixtures.mediaItem),
      ),
      playbackStateProvider.overrideWith(
        (ref) => Stream.value(Fixtures.playbackStatePaused),
      ),
    ],
  );

  // ══════════════════════════════════════════════════════════════════════════
  // QueuePanel
  // ══════════════════════════════════════════════════════════════════════════

  // ── 4. queue_panel_empty ──────────────────────────────────────────────────
  // FakeAudioHandler with queue: [] → upcomingQueue is empty →
  // QueuePanel shows "Nothing up next".
  testGoldenWidget(
    'queue panel — empty',
    'player/queue_panel_empty',
    const QueuePanel(),
    overrides: [
      audioHandlerProvider.overrideWithValue(FakeAudioHandler(queue: [])),
    ],
  );

  // ── 5. queue_panel_populated ──────────────────────────────────────────────
  // FakeAudioHandler with queue items → QueuePanel renders the upcoming list.
  // currentMediaItemProvider is still null (base), so no item shows as current.
  testGoldenWidget(
    'queue panel — populated',
    'player/queue_panel_populated',
    const QueuePanel(),
    overrides: [
      audioHandlerProvider.overrideWithValue(
        FakeAudioHandler(
          queue: [
            (queueIndex: 0, item: Fixtures.mediaItem),
            (
              queueIndex: 1,
              item: MediaItem(
                id: 'local:ddeeff',
                title: 'Autumn Drift',
                artist: 'Maeve',
              ),
            ),
            (
              queueIndex: 2,
              item: MediaItem(
                id: 'local:112233',
                title: 'Neon Pulse',
                artist: 'Volta',
              ),
            ),
          ],
        ),
      ),
    ],
  );
}
