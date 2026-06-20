// ---------------------------------------------------------------------------
// Golden test harness for Sunflower — golden_toolkit integration layer.
//
// Conventions:
//   - All goldens target Android only (Pixel-5-class dark device).
//   - Provider mocking happens at ProviderScope; no Mockito stubs in widget trees.
//   - Fakes live in test/goldens/fakes/fakes.dart (GoldenScreens owns that file).
//   - Fixture data (stable across runs) lives in test/goldens/fakes/fixtures.dart.
//   - Network images never load: fake URLs resolve to nothing, cached_network_image
//     falls back to its placeholder — deterministic, no DNS needed.
//
// ─── Quickstart ─────────────────────────────────────────────────────────────
//
//   // Simplest golden (loaded state, Fixtures data, dark Pixel-5 device):
//   testGoldenWidget(
//     'songs screen — loaded',
//     'library/songs_screen',
//     const SongsScreen(),
//   );
//
//   // Override one provider for a specific state (stale feed):
//   testGoldenWidget(
//     'home screen — stale',
//     'home/home_screen_stale',
//     const HomeScreen(),
//     overrides: [
//       homeFeedProvider.overrideWith((_) async => Fixtures.staleFeed),
//     ],
//   );
//
//   // Loading state — pass a FakeSunflowerApi with null for the field you
//   // want to stay in loading (null → Completer that never resolves):
//   testGoldenWidget(
//     'songs screen — loading',
//     'library/songs_screen_loading',
//     const SongsScreen(),
//     overrides: [
//       sunflowerApiProvider.overrideWithValue(FakeSunflowerApi()),
//     ],
//   );
//
//   // Error state:
//   testGoldenWidget(
//     'songs screen — error',
//     'library/songs_screen_error',
//     const SongsScreen(),
//     overrides: [
//       sunflowerApiProvider.overrideWithValue(
//         FakeSunflowerApi(songsError: Exception('Server down')),
//       ),
//     ],
//   );
//
//   // Queue panel — pass queue items via FakeAudioHandler.queue:
//   testGoldenWidget(
//     'queue panel — populated',
//     'player/queue_panel',
//     const QueuePanel(),
//     overrides: [
//       audioHandlerProvider.overrideWithValue(
//         FakeAudioHandler(queue: [
//           (queueIndex: 0, item: Fixtures.mediaItem),
//         ]),
//       ),
//     ],
//   );
//
// ─── Updating goldens ────────────────────────────────────────────────────────
//
//   flutter test --update-goldens test/goldens/   # from client/
//   make golden-update                            # CI alias
//
// Golden image location: test/goldens/snapshots/<path>.png
//
// Keep the pinned goldenDevice in sync with .github/workflows/goldens.yml
// (pixel_5 AVD, API 34, 393×851 @ 2.75 dpr).
// ---------------------------------------------------------------------------

import 'package:audio_service/audio_service.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:golden_toolkit/golden_toolkit.dart';

import 'package:sunflower/core/api/sunflower_api.dart';
import 'package:sunflower/core/auth/token_store.dart';
import 'package:sunflower/core/player/player_bootstrap.dart';
import 'package:sunflower/core/sync/sync_providers.dart';
import 'package:sunflower/core/ws/ws_providers.dart';

import '../fakes/fakes.dart';
import '../fakes/fixtures.dart';

// Re-export so screen test files need only one import.
export '../fakes/fakes.dart';
export '../fakes/fixtures.dart';
export 'package:sunflower/core/api/sunflower_api.dart'
    show Song, HomeFeed, Playlist;

// ─── Pinned device ───────────────────────────────────────────────────────────
//
// Pixel 5 / Pixel 6a class: 393 × 851 logical px, 2.75 device-pixel ratio,
// dark. Matches the pixel_5 AVD used in CI.

const Device goldenDevice = Device(
  name: 'pixel5_dark',
  size: Size(393, 851),
  devicePixelRatio: 2.75,
  brightness: Brightness.dark,
);

// ─── App theme ───────────────────────────────────────────────────────────────

ThemeData _sunflowerDarkTheme() => ThemeData(
      colorScheme: ColorScheme.fromSeed(
        seedColor: const Color(0xFFFFB300),
        brightness: Brightness.dark,
      ),
      useMaterial3: true,
    );

// ─── Base provider overrides ─────────────────────────────────────────────────
//
// Applied to every golden test's ProviderScope. Intercepts all providers that
// would touch the network, SQLite, secure storage, AudioService, or WebSockets.
//
// Screen tests append extra overrides (e.g. for a specific state) via the
// [overrides] parameter on pumpGolden / testGoldenWidget.

List<Override> _baseOverrides() => [
      // Auth: always appear logged-in.
      tokenProvider.overrideWith((ref) async => 'fake-token'),
      serverUrlProvider.overrideWith((ref) async => 'http://localhost:8080'),

      // API: populated with Fixtures data; no TCP connection opened.
      // For loading/error states, pass FakeSunflowerApi(...) as an override.
      sunflowerApiProvider.overrideWithValue(
        FakeSunflowerApi(
          songs: Fixtures.songs,
          playlists: Fixtures.playlists,
          feed: Fixtures.homeFeed,
          playlist: Fixtures.playlistWithItems('pl-001'),
        ),
      ),

      // Audio: Fake avoids constructing AudioPlayer / just_audio backend.
      // currentMediaItemProvider and playbackStateProvider are overridden
      // directly so handler BehaviorSubjects are never accessed in rendering.
      audioHandlerProvider.overrideWithValue(FakeAudioHandler()),
      currentMediaItemProvider.overrideWith((ref) => Stream.value(null)),
      playbackStateProvider.overrideWith(
        (ref) => Stream.value(PlaybackState()),
      ),

      // WebSocket: disabled — no credentials or real socket needed.
      nowPlayingProvider.overrideWithValue(null),

      // Sync / write-replay: pending = 0, mutations are no-ops.
      pendingCountProvider.overrideWith((ref) => Stream.value(0)),
      bufferedApiProvider.overrideWithValue(FakeBufferedApi()),
    ];

// ─── Core pump helper ────────────────────────────────────────────────────────
//
// Wraps [widget] in a golden_toolkit-configured ProviderScope + MaterialApp,
// pins the device-pixel-ratio, loads fonts, then settles all async providers.
//
// After this call, invoke screenMatchesGolden(tester, path) to capture.

Future<void> pumpGolden(
  WidgetTester tester,
  Widget widget, {
  List<Override> overrides = const [],
}) async {
  // Pin DPR so physical pixel counts match the emulator spec.
  tester.view.devicePixelRatio = goldenDevice.devicePixelRatio;
  addTearDown(tester.view.resetDevicePixelRatio);

  // Load fonts declared in pubspec.yaml for pixel-stable text rendering.
  await loadAppFonts();

  await tester.pumpWidgetBuilder(
    ProviderScope(
      overrides: [..._baseOverrides(), ...overrides],
      child: MaterialApp(
        debugShowCheckedModeBanner: false,
        theme: _sunflowerDarkTheme(),
        darkTheme: _sunflowerDarkTheme(),
        themeMode: ThemeMode.dark,
        home: widget,
      ),
    ),
    surfaceSize: goldenDevice.size,
  );

  // Settle microtasks so FutureProviders with immediate fakes resolve.
  await tester.pumpAndSettle();
}

// ─── Convenience wrapper ─────────────────────────────────────────────────────
//
// Equivalent to: pumpGolden + screenMatchesGolden in a testGoldens block.
// The PNG is written to test/goldens/snapshots/<goldenPath>.png.
//
// [overrides] is merged on top of _baseOverrides(). Screen-specific states
// (loading, error, alternate data) all go here.

void testGoldenWidget(
  String description,
  String goldenPath,
  Widget widget, {
  List<Override> overrides = const [],
}) {
  testGoldens(description, (tester) async {
    await pumpGolden(tester, widget, overrides: overrides);
    await screenMatchesGolden(tester, 'snapshots/$goldenPath');
  });
}

// ─── Multi-state convenience ─────────────────────────────────────────────────
//
// Pumps the same widget under several override sets in one testGoldens block.
// Each scenario label is appended to [goldenBasePath] with an underscore.
//
//   testGoldenScenarios(
//     'songs screen states',
//     'library/songs_screen',
//     widget: const SongsScreen(),
//     scenarios: {
//       'loaded': [],
//       'loading': [sunflowerApiProvider.overrideWithValue(FakeSunflowerApi())],
//       'error': [sunflowerApiProvider.overrideWithValue(
//           FakeSunflowerApi(songsError: Exception('offline')))],
//     },
//   );

void testGoldenScenarios(
  String description,
  String goldenBasePath, {
  required Widget widget,
  required Map<String, List<Override>> scenarios,
}) {
  testGoldens(description, (tester) async {
    for (final entry in scenarios.entries) {
      await pumpGolden(tester, widget, overrides: entry.value);
      await screenMatchesGolden(
        tester,
        'snapshots/${goldenBasePath}_${entry.key}',
      );
    }
  });
}
