import 'dart:convert';

import 'package:drift/drift.dart' show Value;
import 'package:drift/native.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sunflower/core/api/sunflower_api.dart';
import 'package:sunflower/core/auth/token_store.dart';
import 'package:sunflower/core/db/database.dart';
import 'package:sunflower/core/db/database_provider.dart';
import 'package:sunflower/core/recommendations/local_core.dart';
import 'package:sunflower/features/home/home_controller.dart';

void main() {
  late SunflowerDatabase db;

  setUp(() {
    db = SunflowerDatabase.forTesting(NativeDatabase.memory());
  });

  tearDown(() async {
    await db.close();
  });

  test('online home is fetched through recommendation API provider', () async {
    final container = ProviderContainer(
      overrides: [
        sunflowerApiProvider.overrideWithValue(_OfflineHomeApi()),
        localModeProvider.overrideWith((ref) async => false),
        recommendationApiProvider.overrideWithValue(
          _StaticHomeApi(
            const HomeFeed(
              sections: [
                HomeSection(
                  id: 'remote_quick_picks',
                  title: 'Quick picks',
                  kind: 'quick_picks',
                  items: [
                    HomeItem(
                      mediaId: 'yt:remote',
                      title: 'Remote',
                      source: 'yt',
                    ),
                  ],
                ),
              ],
              chips: ['for_you'],
            ),
          ),
        ),
        databaseProvider.overrideWithValue(db),
        localCoreHandleProvider.overrideWith((ref) async => null),
      ],
    );
    addTearDown(container.dispose);

    final feed = await container.read(homeFeedProvider.future);

    expect(feed.sections.single.id, 'remote_quick_picks');
    expect(feed.sections.single.items.single.mediaId, 'yt:remote');
    expect(await db.getHome('home:000'), isNotNull);
  });

  test('offline home prepends local recommendations before cached sections',
      () async {
    await db.putHome(
      'home:000',
      jsonEncode({
        'sections': [
          {
            'id': 'quick_picks',
            'title': 'Quick picks',
            'kind': 'quick_picks',
            'items': [
              {
                'media_id': 'yt:cached',
                'title': 'Cached',
                'source': 'yt',
                'artists': ['Remote'],
              },
            ],
          },
        ],
        'chips': ['for_you'],
        'stale': false,
      }),
    );
    await db.recordPlay(
      RecentPlaysCompanion.insert(
        mediaId: 'local:one',
        title: const Value('One'),
        artistName: const Value('A'),
        source: const Value('local'),
        streamUrl: const Value('http://127.0.0.1/one'),
      ),
    );

    final container = ProviderContainer(
      overrides: [
        recommendationApiProvider.overrideWithValue(_OfflineHomeApi()),
        localModeProvider.overrideWith((ref) async => false),
        databaseProvider.overrideWithValue(db),
        localCoreHandleProvider.overrideWith((ref) async => null),
      ],
    );
    addTearDown(container.dispose);

    final feed = await container.read(homeFeedProvider.future);

    expect(feed.stale, isTrue);
    expect(feed.chips, ['local', 'for_you']);
    expect(feed.sections.map((section) => section.id), [
      'local_quick_picks',
      'quick_picks',
    ]);
    expect(feed.sections.first.items.single.mediaId, 'local:one');
    expect(feed.sections.last.items.single.mediaId, 'yt:cached');
  });

  test('offline home ignores corrupt cache when local recommendations exist',
      () async {
    await db.putHome('home:000', '{not valid json');
    await db.recordPlay(
      RecentPlaysCompanion.insert(
        mediaId: 'local:one',
        title: const Value('One'),
        artistName: const Value('A'),
        source: const Value('local'),
        streamUrl: const Value('http://127.0.0.1/one'),
      ),
    );

    final container = ProviderContainer(
      overrides: [
        recommendationApiProvider.overrideWithValue(_OfflineHomeApi()),
        localModeProvider.overrideWith((ref) async => false),
        databaseProvider.overrideWithValue(db),
        localCoreHandleProvider.overrideWith((ref) async => null),
      ],
    );
    addTearDown(container.dispose);

    final feed = await container.read(homeFeedProvider.future);

    expect(feed.stale, isTrue);
    expect(feed.sections.map((section) => section.id), ['local_quick_picks']);
    expect(feed.sections.single.items.single.mediaId, 'local:one');
  });

  test('local mode home uses local recommendations without remote API',
      () async {
    await db.recordPlay(
      RecentPlaysCompanion.insert(
        mediaId: 'local:one',
        title: const Value('One'),
        artistName: const Value('A'),
        source: const Value('local'),
        streamUrl: Value(Uri.file('/tmp/one.mp3').toString()),
      ),
    );

    final container = ProviderContainer(
      overrides: [
        localModeProvider.overrideWith((ref) async => true),
        recommendationApiProvider.overrideWithValue(_ExplodingHomeApi()),
        databaseProvider.overrideWithValue(db),
        localCoreHandleProvider.overrideWith((ref) async => null),
      ],
    );
    addTearDown(container.dispose);

    final feed = await container.read(homeFeedProvider.future);

    expect(feed.stale, isTrue);
    expect(feed.chips, ['local']);
    expect(feed.sections.map((section) => section.id), ['local_quick_picks']);
    expect(feed.sections.single.items.single.mediaId, 'local:one');
    expect(feed.sections.single.items.single.localPath, '/tmp/one.mp3');
  });
}

class _StaticHomeApi extends SunflowerApi {
  _StaticHomeApi(this.feed)
      : super(baseUrl: 'http://recommendations.test', token: 'token');

  final HomeFeed feed;

  @override
  Future<HomeFeed> home({
    bool hideExplicit = false,
    bool hideVideo = false,
    bool hideShorts = false,
  }) async {
    return feed;
  }
}

class _OfflineHomeApi extends SunflowerApi {
  _OfflineHomeApi() : super(baseUrl: 'http://offline.invalid', token: 'token');

  @override
  Future<HomeFeed> home({
    bool hideExplicit = false,
    bool hideVideo = false,
    bool hideShorts = false,
  }) {
    return Future.error(Exception('offline'));
  }
}

class _ExplodingHomeApi extends SunflowerApi {
  _ExplodingHomeApi()
      : super(baseUrl: 'http://must-not-be-used.invalid', token: 'token');

  @override
  Future<HomeFeed> home({
    bool hideExplicit = false,
    bool hideVideo = false,
    bool hideShorts = false,
  }) {
    throw StateError('remote home should not be called in local mode');
  }
}
