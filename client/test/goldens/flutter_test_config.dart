// Golden test configuration — initialises golden_toolkit for every test in
// this directory. Flutter's test runner discovers this file automatically and
// wraps each test's main() with testExecutable.

import 'dart:async';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:golden_toolkit/golden_toolkit.dart';

const double _goldenDiffTolerance = 0.0002; // 0.02%

Future<void> testExecutable(FutureOr<void> Function() testMain) async {
  final previousComparator = goldenFileComparator;
  goldenFileComparator = _TolerantGoldenFileComparator(
    Uri.file('${Directory.current.path}/test/goldens/flutter_test_config.dart'),
    precisionTolerance: _goldenDiffTolerance,
  );

  try {
    return await GoldenToolkit.runWithConfiguration(
      testMain,
      config: GoldenToolkitConfiguration(
        // Goldens are asserted everywhere. The comparator allows only tiny
        // runner-level antialiasing differences while still catching UI drift.
        skipGoldenAssertion: () => false,
      ),
    );
  } finally {
    goldenFileComparator = previousComparator;
  }
}

class _TolerantGoldenFileComparator extends LocalFileComparator {
  _TolerantGoldenFileComparator(
    super.testFile, {
    required double precisionTolerance,
  })  : assert(
          precisionTolerance >= 0 && precisionTolerance <= 1,
          'precisionTolerance must be between 0 and 1',
        ),
        _precisionTolerance = precisionTolerance;

  final double _precisionTolerance;

  @override
  Future<bool> compare(Uint8List imageBytes, Uri golden) async {
    final result = await GoldenFileComparator.compareLists(
      imageBytes,
      await getGoldenBytes(golden),
    );

    final passed = result.passed || result.diffPercent <= _precisionTolerance;
    if (passed) {
      result.dispose();
      return true;
    }

    final error = await generateFailureOutput(result, golden, basedir);
    result.dispose();
    throw FlutterError(error);
  }
}
