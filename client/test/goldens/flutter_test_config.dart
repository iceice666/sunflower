// Golden test configuration — initialises golden_toolkit for every test in
// this directory. Flutter's test runner discovers this file automatically and
// wraps each test's main() with testExecutable.

import 'dart:async';

import 'package:golden_toolkit/golden_toolkit.dart';

Future<void> testExecutable(FutureOr<void> Function() testMain) async {
  return GoldenToolkit.runWithConfiguration(
    testMain,
    config: GoldenToolkitConfiguration(
      // Goldens are asserted everywhere. The harness installs a tiny pixel
      // tolerance comparator before each screenshot comparison.
      skipGoldenAssertion: () => false,
    ),
  );
}
