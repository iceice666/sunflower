// Golden tests — downloads screen: empty list, active + completed jobs.
//
// downloadJobsProvider is overridden directly so downloadManagerProvider and
// databaseProvider are never initialised. Tap-action callbacks on _ActiveTile
// and _DoneTile call ref.read(downloadManagerProvider) which is lazy and
// never reached during a golden render.
//
// Baseline: flutter test --update-goldens test/goldens/downloads_golden_test.dart
// Compare:  flutter test test/goldens/downloads_golden_test.dart

import 'package:sunflower/core/db/database.dart' show DownloadJob;
import 'package:sunflower/core/downloads/downloads_providers.dart'
    show downloadJobsProvider;
import 'package:sunflower/features/downloads_ui/downloads_screen.dart';

import 'helpers/golden_harness.dart';

// ─── Fixture jobs ────────────────────────────────────────────────────────────

final _activeJob = DownloadJob(
  mediaId: 'local:aabbcc',
  title: 'Sunflower Fields',
  sourceUrl: 'http://localhost:8080/api/v1/library/songs/local:aabbcc/stream',
  status: 'running',
  totalBytes: 10000000,
  receivedBytes: 3500000,
  updatedAt: DateTime(2025, 6, 1),
);

final _failedJob = DownloadJob(
  mediaId: 'local:112233',
  title: 'Neon Pulse',
  sourceUrl: 'http://localhost:8080/api/v1/library/songs/local:112233/stream',
  status: 'failed',
  totalBytes: 0,
  receivedBytes: 0,
  error: 'Connection timeout',
  updatedAt: DateTime(2025, 6, 1),
);

final _doneJob = DownloadJob(
  mediaId: 'local:ddeeff',
  title: 'Autumn Drift',
  sourceUrl: 'http://localhost:8080/api/v1/library/songs/local:ddeeff/stream',
  status: 'completed',
  totalBytes: 8200000,
  receivedBytes: 8200000,
  updatedAt: DateTime(2025, 6, 1),
);

// ─── Tests ───────────────────────────────────────────────────────────────────

void main() {
  // ── 4. downloads_empty ────────────────────────────────────────────────────
  // No jobs at all; screen shows "No downloads".
  testGoldenWidget(
    'downloads screen — empty',
    'downloads/downloads_empty',
    const DownloadsScreen(),
    overrides: [
      downloadJobsProvider.overrideWith(
        (ref) => Stream.value(const <DownloadJob>[]),
      ),
    ],
  );

  // ── 5. downloads_screen ───────────────────────────────────────────────────
  // Mixed state: one running job (with progress bar), one failed job (error
  // message), and one completed job (download_done icon + remove button).
  testGoldenWidget(
    'downloads screen — active, failed, and completed jobs',
    'downloads/downloads_screen',
    const DownloadsScreen(),
    overrides: [
      downloadJobsProvider.overrideWith(
        (ref) => Stream.value([_activeJob, _failedJob, _doneJob]),
      ),
    ],
  );
}
