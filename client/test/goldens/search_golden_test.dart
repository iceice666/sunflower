// Golden tests — Search screen: empty, results, error.

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:golden_toolkit/golden_toolkit.dart'
    show testGoldens, screenMatchesGolden;

import 'package:sunflower/core/api/sunflower_api.dart'
    show sunflowerApiProvider;
import 'package:sunflower/features/search/search_screen.dart';

import 'helpers/golden_harness.dart';

void main() {
  testGoldenWidget(
    'search screen — empty prompt',
    'search/search_empty',
    const SearchScreen(),
  );

  testGoldens('search screen — results', (tester) async {
    await pumpGolden(tester, const SearchScreen());
    await tester.enterText(find.byType(TextField), 'sun');
    await tester.pump(const Duration(milliseconds: 350));
    tester.testTextInput.hide();
    FocusManager.instance.primaryFocus?.unfocus();
    await tester.pump(const Duration(milliseconds: 40));
    await screenMatchesGolden(tester, 'snapshots/search/search_results');
  });

  testGoldens('search screen — unavailable', (tester) async {
    await pumpGolden(
      tester,
      const SearchScreen(),
      overrides: [
        sunflowerApiProvider.overrideWithValue(
          FakeSunflowerApi(searchError: Exception('search unavailable')),
        ),
      ],
    );
    await tester.enterText(find.byType(TextField), 'sun');
    await tester.pump(const Duration(milliseconds: 350));
    tester.testTextInput.hide();
    FocusManager.instance.primaryFocus?.unfocus();
    await tester.pump(const Duration(milliseconds: 60));
    await screenMatchesGolden(tester, 'snapshots/search/search_error');
  });
}
