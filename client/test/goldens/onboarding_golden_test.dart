// Golden tests — onboarding / server setup.
//
// Baseline: flutter test --update-goldens test/goldens/onboarding_golden_test.dart
// Compare:  flutter test test/goldens/onboarding_golden_test.dart

import 'package:sunflower/features/onboarding/server_setup_screen.dart';

import 'helpers/golden_harness.dart';

void main() {
  // ── 1. server_setup ───────────────────────────────────────────────────────
  // Initial form: pre-filled server URL, Connect button, no error message.
  // ServerSetupScreen has no provider dependencies; base overrides are no-ops.
  testGoldenWidget(
    'onboarding — server setup initial form',
    'onboarding/server_setup',
    const ServerSetupScreen(),
  );
}
