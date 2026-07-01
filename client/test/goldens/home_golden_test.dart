// Golden tests — home feed screen: sections/chips, stale (offline), error.
//
// HomeScreen uses homeFeedProvider which reads recommendationApiProvider and
// databaseProvider (for the stale-cache write). All three tests override
// homeFeedProvider directly so the database is never touched.
//
// Baseline: flutter test --update-goldens test/goldens/home_golden_test.dart
// Compare:  flutter test test/goldens/home_golden_test.dart

import 'package:sunflower/features/home/home_controller.dart'
    show homeFeedProvider;
import 'package:sunflower/features/home/home_screen.dart';

import 'helpers/golden_harness.dart';

void main() {
  // ── 1. home_sections ──────────────────────────────────────────────────────
  // Normal state: 2 sections (Quick Picks, Daily Discover) with 3 chips.
  // SyncStatusWidget inside _FeedBody shows nothing (pending = 0, drops = 0).
  testGoldenWidget(
    'home screen — sections and chips',
    'home/home_sections',
    const HomeScreen(),
    overrides: [
      homeFeedProvider.overrideWith((_) async => Fixtures.homeFeed),
    ],
  );

  // ── 2. home_stale ─────────────────────────────────────────────────────────
  // Cold-start offline state: feed.stale = true triggers _StaleBanner
  // ("Showing saved recommendations — couldn't reach the server.").
  testGoldenWidget(
    'home screen — stale feed (offline banner)',
    'home/home_stale',
    const HomeScreen(),
    overrides: [
      homeFeedProvider.overrideWith((_) async => Fixtures.staleFeed),
    ],
  );

  // ── 3. home_error ─────────────────────────────────────────────────────────
  // Server unreachable and no stale cache: FutureProvider resolves to error.
  // _ErrorView renders "Could not load recommendations." with Retry button.
  testGoldenWidget(
    'home screen — error (no cache)',
    'home/home_error',
    const HomeScreen(),
    overrides: [
      homeFeedProvider.overrideWith(
        (_) => Future<HomeFeed>.error(Exception('offline')),
      ),
    ],
  );
}
