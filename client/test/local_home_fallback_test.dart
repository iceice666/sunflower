import 'package:drift/drift.dart' show Value;
import 'package:drift/native.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sunflower/core/bridge/api.dart' as bridge;
import 'package:sunflower/core/db/database.dart';
import 'package:sunflower/core/recommendations/local_home_fallback.dart';

void main() {
  late SunflowerDatabase db;

  setUp(() {
    db = SunflowerDatabase.forTesting(NativeDatabase.memory());
  });

  tearDown(() async {
    await db.close();
  });

  test('recordPlay increments the local stat snapshot play count', () async {
    final play = RecentPlaysCompanion.insert(
      mediaId: 'local:one',
      title: const Value('One'),
      artistName: const Value('A'),
      source: const Value('local'),
      streamUrl: const Value('http://127.0.0.1/one'),
      durationMs: const Value(123),
    );

    await db.recordPlay(play);
    await db.recordPlay(play);

    final rows = await db.recentPlays_();
    expect(rows, hasLength(1));
    expect(rows.single.playCount, 2);
  });

  test('local fallback builds a stale playable Home feed from recent plays',
      () async {
    await db.recordPlay(
      RecentPlaysCompanion.insert(
        mediaId: 'local:one',
        title: const Value('One'),
        artistName: const Value('A'),
        source: const Value('local'),
        streamUrl: const Value('http://127.0.0.1/one'),
        durationMs: const Value(123),
      ),
    );
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

    final feed = await const LocalHomeFallback(useRustBridge: false).build(db);

    expect(feed, isNotNull);
    expect(feed!.stale, isTrue);
    expect(feed.sections, hasLength(1));
    expect(feed.sections.single.id, 'local_quick_picks');
    expect(feed.sections.single.items.map((item) => item.mediaId), [
      'local:one',
    ]);
  });

  test('local fallback includes downloaded remote tracks', () async {
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

    final feed = await const LocalHomeFallback(useRustBridge: false).build(db);

    expect(feed, isNotNull);
    expect(feed!.sections.single.items.single.mediaId, 'yt:remote');
  });

  test('local stats snapshot excludes network-only remote tracks', () async {
    await db.recordPlay(
      RecentPlaysCompanion.insert(
        mediaId: 'local:one',
        title: const Value('One'),
        artistName: const Value('A'),
        source: const Value('local'),
        streamUrl: const Value('http://127.0.0.1/one'),
      ),
    );
    await db.recordPlay(
      RecentPlaysCompanion.insert(
        mediaId: 'yt:network',
        title: const Value('Network'),
        artistName: const Value('B'),
        source: const Value('yt'),
        streamUrl: const Value('https://googlevideo.example/stream'),
      ),
    );
    await db.recordPlay(
      RecentPlaysCompanion.insert(
        mediaId: 'yt:downloaded',
        title: const Value('Downloaded'),
        artistName: const Value('C'),
        source: const Value('yt'),
        streamUrl: const Value(null),
      ),
    );
    await db.completeDownload(
      DownloadedTracksCompanion.insert(
        mediaId: 'yt:downloaded',
        localPath: '/data/downloads/downloaded.audio',
      ),
    );

    bridge.LocalStatsSnapshotDto? captured;
    final feed = await LocalHomeFallback(
      ranker: ({
        required candidates,
        required stats,
        required limit,
      }) async {
        captured = stats;
        return candidates;
      },
    ).build(db);

    expect(feed, isNotNull);
    final byId = {
      for (final track in captured!.tracks) track.mediaId: track,
    };
    expect(byId['local:one']!.downloaded, isFalse);
    expect(byId['local:one']!.localAvailable, isTrue);
    expect(byId, isNot(contains('yt:network')));
    expect(byId['yt:downloaded']!.downloaded, isTrue);
    expect(byId['yt:downloaded']!.localAvailable, isTrue);
  });

  test('local fallback can rank with a Rust-core stats snapshot', () async {
    await db.recordPlay(
      RecentPlaysCompanion.insert(
        mediaId: 'local:one',
        title: const Value('One'),
        artistName: const Value('A'),
        source: const Value('local'),
        streamUrl: const Value('http://127.0.0.1/one'),
        durationMs: const Value(123),
      ),
    );

    bridge.LocalStatsSnapshotDto? captured;
    final feed = await LocalHomeFallback(
      statsLoader: ({required recentLimit}) async {
        expect(recentLimit, 10);
        return bridge.LocalStatsSnapshotDto(
          generatedAtMs: 1,
          tracks: [
            bridge.TrackStatsDto(
              mediaId: 'local:one',
              playCount: 7,
              skipCount: 1,
              completionCount: 6,
              impressionCount: 2,
              liked: true,
              downloaded: false,
              localAvailable: true,
              lastPlayedAtMs: 1,
            ),
          ],
          recentMediaIds: const ['local:one'],
          recentArtistNames: const ['A'],
        );
      },
      ranker: ({
        required candidates,
        required stats,
        required limit,
      }) async {
        captured = stats;
        return candidates;
      },
    ).build(db);

    expect(feed, isNotNull);
    expect(captured!.tracks.single.playCount, 7);
    expect(captured!.tracks.single.liked, isTrue);
  });
}
