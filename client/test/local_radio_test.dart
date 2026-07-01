import 'package:drift/drift.dart' show Value;
import 'package:drift/native.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sunflower/core/db/database.dart';
import 'package:sunflower/core/player/local_radio.dart';

void main() {
  late SunflowerDatabase db;
  late LocalRadio radio;

  setUp(() {
    db = SunflowerDatabase.forTesting(NativeDatabase.memory());
    radio = LocalRadio(db);
  });

  tearDown(() async {
    await db.close();
  });

  test('uses completed downloads when recent remote play has no stream URL',
      () async {
    await db.recordPlay(
      RecentPlaysCompanion.insert(
        mediaId: 'yt:remote',
        title: const Value('Remote'),
        artistName: const Value('B'),
        source: const Value('yt'),
        streamUrl: const Value(null),
        durationMs: const Value(456),
      ),
    );
    await db.completeDownload(
      DownloadedTracksCompanion.insert(
        mediaId: 'yt:remote',
        localPath: '/data/downloads/remote.audio',
      ),
    );

    final entries = await radio.fromRecentPlays();

    expect(entries, hasLength(1));
    expect(entries.single.item.mediaId, 'yt:remote');
    expect(entries.single.url, startsWith('file://'));
  });

  test('skips remote recent plays that are neither cached nor downloaded',
      () async {
    await db.recordPlay(
      RecentPlaysCompanion.insert(
        mediaId: 'yt:missing',
        title: const Value('Missing'),
        source: const Value('yt'),
        streamUrl: const Value(null),
      ),
    );

    expect(await radio.fromRecentPlays(), isEmpty);
  });

  test('skips remote recent plays with only a transient network stream URL',
      () async {
    await db.recordPlay(
      RecentPlaysCompanion.insert(
        mediaId: 'yt:network',
        title: const Value('Network'),
        source: const Value('yt'),
        streamUrl: const Value('https://googlevideo.example/stream'),
      ),
    );

    expect(await radio.fromRecentPlays(), isEmpty);
  });
}
