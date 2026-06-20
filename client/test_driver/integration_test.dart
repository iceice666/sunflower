// Flutter-drive host driver for integration_test/visual_smoke_test.dart.
//
// Receives screenshots over the integration_test screenshot channel and the
// test's reportData, then writes both to build/smoke-artifacts/ (relative to
// the directory `flutter drive` runs from, i.e. client/).
import 'dart:convert';
import 'dart:io';

import 'package:integration_test/integration_test_driver_extended.dart';

const _artifactDir = 'build/smoke-artifacts';

Future<void> main() async {
  await integrationDriver(
    onScreenshot: (
      String name,
      List<int> bytes, [
      Map<String, Object?>? args,
    ]) async {
      final dir = Directory(_artifactDir)..createSync(recursive: true);
      File('${dir.path}/$name.png').writeAsBytesSync(bytes);
      stdout.writeln(
        '[driver] wrote $_artifactDir/$name.png (${bytes.length} B)',
      );
      return true;
    },
    responseDataCallback: (Map<String, dynamic>? data) async {
      final snapshot = data?['t10_admin_snapshot'];
      if (snapshot == null) return;
      final dir = Directory(_artifactDir)..createSync(recursive: true);
      File('${dir.path}/t10_admin_snapshot.json').writeAsStringSync(
        const JsonEncoder.withIndent('  ').convert(snapshot),
      );
      stdout.writeln('[driver] wrote $_artifactDir/t10_admin_snapshot.json');
    },
  );
}
