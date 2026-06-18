import 'package:drift/drift.dart' show Value;
import 'package:drift/native.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sunflower/core/db/database.dart';
import 'package:sunflower/core/player/source_resolver.dart';

// Verifies the M6 prefer-local rule: when a media id is downloaded, the resolver
// returns the local file URI; otherwise it falls back to the network URL.
//
// Uses an in-memory Drift database via SunflowerDatabase.forTesting — no
// platform plugins, so this runs under `flutter test`.
void main() {
  late SunflowerDatabase db;
  late SourceResolver resolver;

  setUp(() {
    db = SunflowerDatabase.forTesting(NativeDatabase.memory());
    resolver = SourceResolver(db);
  });

  tearDown(() async {
    await db.close();
  });

  test('falls back to network URL when not downloaded', () async {
    final uri = await resolver.resolve('yt:abc', 'https://stream/abc');
    expect(uri, 'https://stream/abc');
    expect(await resolver.localUriFor('yt:abc'), isNull);
  });

  test('prefers the local file when downloaded', () async {
    await db.completeDownload(
      DownloadedTracksCompanion.insert(
        mediaId: 'local:xyz',
        localPath: '/data/downloads/local_xyz.audio',
        bytes: const Value(1024),
      ),
    );

    final local = await resolver.localUriFor('local:xyz');
    expect(local, isNotNull);
    expect(local, startsWith('file://'));

    // resolve() must choose local over the provided network URL.
    final chosen = await resolver.resolve('local:xyz', 'https://stream/xyz');
    expect(chosen, local);
  });
}
