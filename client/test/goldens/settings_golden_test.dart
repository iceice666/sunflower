// Golden tests — settings: full screen, YouTube credentials, sync status,
// crossfade setting.
//
// SettingsScreen and CrossfadeSetting read crossfadeProvider, which loads
// from SharedPreferences. Tests for those widgets use raw testGoldens so that
// SharedPreferences.setMockInitialValues can be called before pumpGolden runs.
//
// SyncStatusWidget does not read crossfadeProvider, so testGoldenWidget is
// sufficient for that standalone test.
//
// Baseline: flutter test --update-goldens test/goldens/settings_golden_test.dart
// Compare:  flutter test test/goldens/settings_golden_test.dart

import 'package:golden_toolkit/golden_toolkit.dart'
    show testGoldens, screenMatchesGolden;
import 'package:shared_preferences/shared_preferences.dart';

import 'package:sunflower/core/sync/sync_providers.dart'
    show pendingCountProvider, bufferedApiProvider;
import 'package:sunflower/features/settings/crossfade_setting.dart';
import 'package:sunflower/features/settings/settings_screen.dart';
import 'package:sunflower/features/settings/sync_status_widget.dart';

import 'helpers/golden_harness.dart';

void main() {
  // ── 6. settings_screen ────────────────────────────────────────────────────
  // Full settings screen: AppBar "Settings", SyncStatusWidget hidden (0
  // pending), YouTube credentials, and CrossfadeSetting in disabled state
  // (default prefs).
  testGoldens('settings screen — default', (tester) async {
    SharedPreferences.setMockInitialValues({});
    await pumpGolden(tester, const SettingsScreen());
    await screenMatchesGolden(tester, 'snapshots/settings/settings_screen');
  });

  // ── 7. sync_status_pending ────────────────────────────────────────────────
  // SyncStatusWidget with pending mutations and overflow drops:
  //   • Title:    "5 pending"
  //   • Subtitle: "2 dropped (buffer full)"  — shown in error colour
  //   • Trailing: TextButton "Retry now"
  // Overrides replace the base values (last-wins in the merged list).
  testGoldenWidget(
    'sync status — pending mutations and overflow drops',
    'settings/sync_status_pending',
    const SyncStatusWidget(),
    overrides: [
      pendingCountProvider.overrideWith((ref) => Stream.value(5)),
      bufferedApiProvider.overrideWithValue(FakeBufferedApi(drops: 2)),
    ],
  );

  // ── 8. crossfade_disabled ─────────────────────────────────────────────────
  // Default prefs: enabled = false, seconds = 6.
  // Only the SwitchListTile is shown; the duration slider is hidden.
  testGoldens('crossfade setting — disabled', (tester) async {
    SharedPreferences.setMockInitialValues({});
    await pumpGolden(tester, const CrossfadeSetting());
    await screenMatchesGolden(tester, 'snapshots/settings/crossfade_disabled');
  });

  // ── 9. crossfade_enabled ──────────────────────────────────────────────────
  // Prefs: enabled = true, seconds = 8.
  // SwitchListTile is on; the duration ListTile with slider (1–12 s) appears.
  testGoldens('crossfade setting — enabled with slider', (tester) async {
    SharedPreferences.setMockInitialValues({
      'crossfade_enabled': true,
      'crossfade_seconds': 8,
    });
    await pumpGolden(tester, const CrossfadeSetting());
    await screenMatchesGolden(tester, 'snapshots/settings/crossfade_enabled');
  });
}
