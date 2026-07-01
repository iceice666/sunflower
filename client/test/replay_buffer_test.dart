import 'package:dio/dio.dart';
import 'package:drift/drift.dart' show Value;
import 'package:drift/native.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sunflower/core/api/api_client.dart';
import 'package:sunflower/core/db/database.dart';
import 'package:sunflower/core/sync/eviction.dart';
import 'package:sunflower/core/sync/replay_buffer.dart';
import 'package:sunflower/core/sync/retry_policy.dart';

// Records every request the buffer replays so we can assert order + idempotency.
class _RecordingInterceptor extends Interceptor {
  final List<String> keys = [];
  final List<String> paths = [];
  final List<Map<String, dynamic>?> bodies = [];
  final List<int> statusCodes = [];
  int failFirstN = 0;
  _RecordingInterceptor();

  @override
  void onRequest(RequestOptions options, RequestInterceptorHandler handler) {
    keys.add(options.headers['Idempotency-Key'] as String);
    paths.add(options.path);
    final data = options.data;
    bodies.add(data is Map ? Map<String, dynamic>.from(data) : null);
    if (failFirstN > 0) {
      failFirstN--;
      handler.reject(DioException(
        requestOptions: options,
        error: 'simulated failure',
      ));
      return;
    }
    final statusCode = statusCodes.isEmpty ? 200 : statusCodes.removeAt(0);
    final response = Response(
      requestOptions: options,
      statusCode: statusCode,
      data: statusCode >= 400 ? {'error': 'simulated'} : null,
    );
    if (statusCode >= 400) {
      handler.reject(DioException.badResponse(
        statusCode: statusCode,
        requestOptions: options,
        response: response,
      ));
      return;
    }
    handler.resolve(response);
  }
}

void main() {
  group('RetryPolicy', () {
    const p = RetryPolicy();
    test('matches the 5s/30s/5m/30m/2h schedule', () {
      expect(p.delayMs(1), 5 * 1000);
      expect(p.delayMs(2), 30 * 1000);
      expect(p.delayMs(3), 5 * 60 * 1000);
      expect(p.delayMs(4), 30 * 60 * 1000);
      expect(p.delayMs(5), 2 * 60 * 60 * 1000);
      // Beyond schedule saturates at the 2h cap.
      expect(p.delayMs(9), 2 * 60 * 60 * 1000);
      expect(p.delayMs(0), 0);
    });
  });

  group('Eviction', () {
    test('priority ordering: like > default > impression', () {
      expect(Eviction.priorityFor('like'),
          greaterThan(Eviction.priorityFor('playlist_add')));
      expect(Eviction.priorityFor('playlist_add'),
          greaterThan(Eviction.priorityFor('impression')));
      expect(Eviction.priorityFor('event'),
          equals(Eviction.priorityFor('playlist_add')));
    });
  });

  group('ReplayBuffer', () {
    late SunflowerDatabase db;
    late Dio dio;
    late _RecordingInterceptor rec;
    var clock = 1000;

    setUp(() {
      db = SunflowerDatabase.forTesting(NativeDatabase.memory());
      rec = _RecordingInterceptor();
      dio = Dio(BaseOptions(baseUrl: 'http://test'))..interceptors.add(rec);
    });

    tearDown(() async => db.close());

    ReplayBuffer buildBuffer() => ReplayBuffer(
          dio: dio,
          db: db,
          nowMs: () => clock++,
        );

    test('drains queued mutations in client-clock order', () async {
      final buffer = buildBuffer();
      await buffer
          .enqueue(kind: 'like', method: 'POST', path: '/a', body: {'i': 1});
      await buffer
          .enqueue(kind: 'like', method: 'POST', path: '/b', body: {'i': 2});
      await buffer
          .enqueue(kind: 'like', method: 'POST', path: '/c', body: {'i': 3});
      await buffer.drain();

      expect(rec.paths, ['/a', '/b', '/c']);
      // All confirmed → purged → zero pending.
      expect(await db.pendingCount(), 0);
    });

    test('re-draining confirmed mutations does not re-send (idempotent)',
        () async {
      final buffer = buildBuffer();
      await buffer.enqueue(kind: 'like', method: 'POST', path: '/a');
      await buffer.drain();
      final sentAfterFirst = rec.paths.length;

      await buffer.drain(); // nothing left to send
      expect(rec.paths.length, sentAfterFirst);
    });

    test('failed mutation is rescheduled and retried on next drain', () async {
      rec.failFirstN = 1; // first attempt fails
      final buffer = buildBuffer();
      await buffer.enqueue(kind: 'like', method: 'POST', path: '/a');
      await buffer.drain(); // attempt 1 fails → rescheduled

      // Not confirmed yet.
      expect(await db.pendingCount(), 1);

      // Advance the clock past the backoff and drain again → succeeds.
      clock += 10 * 1000;
      await buffer.drain();
      expect(await db.pendingCount(), 0);
    });

    test('re-enqueue with the same key preserves the original mutation',
        () async {
      rec.failFirstN = 1;
      final buffer = buildBuffer();
      const key = '018f3f27-0000-7000-8000-000000000123';

      await buffer.enqueue(
        kind: 'event',
        method: 'POST',
        path: '/api/v1/events',
        idempotencyKey: key,
        body: {'original': true},
      );
      expect(await db.pendingCount(), 1);

      await buffer.enqueue(
        kind: 'like',
        method: 'POST',
        path: '/api/v1/likes',
        idempotencyKey: key,
        body: {'changed': true},
      );
      clock += 10 * 1000;
      await buffer.drain();

      expect(rec.keys, [key, key]);
      expect(rec.paths, ['/api/v1/events', '/api/v1/events']);
      expect(rec.bodies, [
        {'original': true},
        {'original': true},
      ]);
      expect(await db.pendingCount(), 0);
    });

    test('permanent 4xx mutation is discarded and drain continues', () async {
      final buffer = buildBuffer();
      rec.statusCodes.addAll([400, 200]);

      await db.enqueueMutation(PendingMutationsCompanion.insert(
        idempotencyKey: 'bad-key',
        kind: 'event',
        method: 'POST',
        path: '/api/v1/events',
        bodyJson: const Value('{}'),
        clientClock: 1,
      ));
      await db.enqueueMutation(PendingMutationsCompanion.insert(
        idempotencyKey: 'good-key',
        kind: 'like',
        method: 'POST',
        path: '/api/v1/likes',
        bodyJson: const Value('{}'),
        clientClock: 2,
      ));

      await buffer.drain();

      expect(rec.paths, ['/api/v1/events', '/api/v1/likes']);
      expect(await db.pendingCount(), 0);
    });

    test('empty bodyJson replays as an empty DELETE body', () async {
      final buffer = buildBuffer();
      await db.enqueueMutation(PendingMutationsCompanion.insert(
        idempotencyKey: 'delete-key',
        kind: 'playlist_delete',
        method: 'DELETE',
        path: '/api/v1/playlists/018f3f27-0000-7000-8000-000000000001',
        clientClock: 1,
      ));

      await buffer.drain();

      expect(rec.paths, [
        '/api/v1/playlists/018f3f27-0000-7000-8000-000000000001',
      ]);
      expect(rec.bodies.single, isNull);
      expect(await db.pendingCount(), 0);
    });

    test('buffered download registry mutations replay with stable routes',
        () async {
      final api = BufferedApiClient(buildBuffer());

      await api.registerDownload(
        deviceId: '018f3f27-0000-7000-8000-000000000001',
        mediaId: 'yt:abc/def',
        localPath: '/data/downloads/yt_abc.audio',
        bytes: 123,
      );
      await api.removeDownload(
        '018f3f27-0000-7000-8000-000000000001',
        'yt:abc/def',
      );

      expect(rec.paths, [
        '/api/v1/devices/018f3f27-0000-7000-8000-000000000001/downloads',
        '/api/v1/devices/018f3f27-0000-7000-8000-000000000001/downloads/yt%3Aabc%2Fdef',
      ]);
    });

    test('buffered path media ids are encoded as single route segments',
        () async {
      final api = BufferedApiClient(buildBuffer());

      await api.removePlaylistItem(
        '018f3f27-0000-7000-8000-000000000001',
        'local:abc/def',
      );

      expect(rec.paths, [
        '/api/v1/playlists/018f3f27-0000-7000-8000-000000000001/items/local%3Aabc%2Fdef',
      ]);
    });

    test('buffered playlist creation replays with the legacy title body',
        () async {
      final api = BufferedApiClient(buildBuffer());
      const key = '018f3f27-0000-7000-8000-000000000124';

      final usedKey = await api.createPlaylist(
        'Road Mix',
        idempotencyKey: key,
      );

      expect(usedKey, key);
      expect(rec.keys, [key]);
      expect(rec.paths, ['/api/v1/playlists']);
      expect(rec.bodies.single, {'title': 'Road Mix'});
    });

    test('buffered impressions replay as low-priority mutations', () async {
      final api = BufferedApiClient(buildBuffer());

      await api.logImpressions([
        {
          'section_id': 'quick_picks',
          'source': 'local',
          'seed_id': '',
          'media_id': 'local:one',
          'position': 0,
        },
      ]);

      expect(rec.paths, ['/api/v1/impressions']);
      expect(await db.pendingCount(), 0);
    });

    test('buffered likes preserve the client action timestamp', () async {
      final api = BufferedApiClient(buildBuffer());
      final occurredAt = DateTime.utc(2026, 7, 1, 1, 2, 3, 456);

      await api.like('local:one', liked: true, occurredAt: occurredAt);

      expect(rec.paths, ['/api/v1/likes']);
      expect(rec.bodies.single, {
        'media_id': 'local:one',
        'liked': true,
        'occurred_at': '2026-07-01T01:02:03.456Z',
      });
    });

    test('buffered feedback can reuse a local core event id as idempotency key',
        () async {
      final api = BufferedApiClient(buildBuffer());
      const eventId = '018f3f27-0000-7000-8000-000000000099';

      final key = await api.like(
        'local:one',
        liked: true,
        idempotencyKey: eventId,
      );

      expect(key, eventId);
      expect(rec.keys, [eventId]);
    });

    test('buffered scrobbles send event_id matching the idempotency key',
        () async {
      final api = BufferedApiClient(buildBuffer());
      const eventId = '018f3f27-0000-7000-8000-000000000100';

      final key = await api.scrobble(
        mediaId: 'local:one',
        queueId: '',
        totalPlayedMs: 60000,
        durationMs: 120000,
        occurredAt: DateTime.utc(2026, 7, 1, 1, 2, 3),
        idempotencyKey: eventId,
      );

      expect(key, eventId);
      expect(rec.keys, [eventId]);
      expect(rec.bodies.single, {
        'events': [
          {
            'event_id': eventId,
            'kind': 'play',
            'media_id': 'local:one',
            'queue_id': '',
            'total_played_ms': 60000,
            'duration_ms': 120000,
            'occurred_at': '2026-07-01T01:02:03.000Z',
          },
        ],
      });
    });

    test('buffered playback events use one UUIDv7 for header and event id',
        () async {
      final api = BufferedApiClient(buildBuffer());

      final key = await api.playbackEvent(
        kind: 'skip',
        mediaId: 'local:one',
        queueId: '018f3f27-0000-7000-8000-000000000010',
        totalPlayedMs: 12000,
        durationMs: 180000,
        reason: 'manual_skip',
        occurredAt: DateTime.utc(2026, 7, 1, 1, 2, 3, 456),
      );

      expect(key, rec.keys.single);
      expect(
        key,
        matches(
          RegExp(
            r'^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$',
          ),
        ),
      );
      expect(rec.bodies.single, {
        'events': [
          {
            'event_id': key,
            'kind': 'skip',
            'media_id': 'local:one',
            'queue_id': '018f3f27-0000-7000-8000-000000000010',
            'total_played_ms': 12000,
            'duration_ms': 180000,
            'occurred_at': '2026-07-01T01:02:03.456Z',
            'reason': 'manual_skip',
          },
        ],
      });
    });

    test('duplicate enqueue at cap does not evict another mutation', () async {
      final buffer = buildBuffer();
      const duplicateKey = 'duplicate-key';
      const victimKey = 'victim-impression-key';
      const futureAttempt = 1 << 50;

      await db.enqueueMutation(PendingMutationsCompanion.insert(
        idempotencyKey: duplicateKey,
        kind: 'like',
        method: 'POST',
        path: '/api/v1/likes',
        clientClock: 1,
        nextAttemptAt: const Value(futureAttempt),
        priority: Value(Eviction.priorityFor('like')),
      ));
      await db.enqueueMutation(PendingMutationsCompanion.insert(
        idempotencyKey: victimKey,
        kind: 'impression',
        method: 'POST',
        path: '/api/v1/impressions',
        clientClock: 2,
        nextAttemptAt: const Value(futureAttempt),
        priority: Value(Eviction.priorityFor('impression')),
      ));
      for (var i = 0; i < Eviction.bufferCap - 2; i++) {
        await db.enqueueMutation(PendingMutationsCompanion.insert(
          idempotencyKey: 'filler-$i',
          kind: 'event',
          method: 'POST',
          path: '/api/v1/events',
          clientClock: 3 + i,
          nextAttemptAt: const Value(futureAttempt),
          priority: Value(Eviction.priorityFor('event')),
        ));
      }
      expect(await db.pendingCount(), Eviction.bufferCap);

      final key = await buffer.enqueue(
        kind: 'like',
        method: 'POST',
        path: '/api/v1/likes',
        idempotencyKey: duplicateKey,
        body: {'media_id': 'local:one', 'liked': true},
      );

      expect(key, duplicateKey);
      expect(rec.paths, isEmpty);
      expect(await db.pendingCount(), Eviction.bufferCap);
      expect(await db.hasMutation(victimKey), isTrue);
    });

    test('overflow eviction drops impressions before playback events',
        () async {
      await db.enqueueMutation(PendingMutationsCompanion.insert(
        idempotencyKey: 'event-key',
        kind: 'event',
        method: 'POST',
        path: '/api/v1/events',
        clientClock: 1,
        priority: Value(Eviction.priorityFor('event')),
      ));
      await db.enqueueMutation(PendingMutationsCompanion.insert(
        idempotencyKey: 'impression-key',
        kind: 'impression',
        method: 'POST',
        path: '/api/v1/impressions',
        clientClock: 2,
        priority: Value(Eviction.priorityFor('impression')),
      ));
      await db.enqueueMutation(PendingMutationsCompanion.insert(
        idempotencyKey: 'like-key',
        kind: 'like',
        method: 'POST',
        path: '/api/v1/likes',
        clientClock: 3,
        priority: Value(Eviction.priorityFor('like')),
      ));

      expect(await db.evictOldestLowPriority(), 'impression-key');
      expect(await db.evictOldestLowPriority(), 'event-key');
      expect(await db.evictOldestLowPriority(), 'like-key');
    });
  });

  group('DirectRecommendationFeedbackClient', () {
    late Dio dio;
    late _RecordingInterceptor rec;

    setUp(() {
      rec = _RecordingInterceptor();
      dio = Dio(BaseOptions(baseUrl: 'http://recommendations.test'))
        ..interceptors.add(rec);
    });

    test('posts playback feedback directly with the local event id', () async {
      final api = DirectRecommendationFeedbackClient(dio: dio);
      const eventId = '018f3f27-0000-7000-8000-000000000200';

      final key = await api.playbackEvent(
        kind: 'skip',
        mediaId: 'yt:skip',
        queueId: '018f3f27-0000-7000-8000-000000000010',
        totalPlayedMs: 12000,
        durationMs: 42000,
        reason: 'user_skip',
        occurredAt: DateTime.utc(2026, 7, 1, 1, 2, 3),
        idempotencyKey: eventId,
      );

      expect(key, eventId);
      expect(rec.keys, [eventId]);
      expect(rec.paths, ['/api/v1/events']);
      expect(rec.bodies.single, {
        'events': [
          {
            'event_id': eventId,
            'kind': 'skip',
            'media_id': 'yt:skip',
            'queue_id': '018f3f27-0000-7000-8000-000000000010',
            'total_played_ms': 12000,
            'duration_ms': 42000,
            'occurred_at': '2026-07-01T01:02:03.000Z',
            'reason': 'user_skip',
          },
        ],
      });
    });

    test('posts preference feedback directly with the local event id',
        () async {
      final api = DirectRecommendationFeedbackClient(dio: dio);
      const eventId = '018f3f27-0000-7000-8000-000000000201';

      final key = await api.like(
        'yt:liked',
        liked: false,
        occurredAt: DateTime.utc(2026, 7, 1, 1, 2, 3),
        idempotencyKey: eventId,
      );

      expect(key, eventId);
      expect(rec.keys, [eventId]);
      expect(rec.paths, ['/api/v1/likes']);
      expect(rec.bodies.single, {
        'media_id': 'yt:liked',
        'liked': false,
        'occurred_at': '2026-07-01T01:02:03.000Z',
      });
    });

    test('posts impression feedback directly with the local event id',
        () async {
      final api = DirectRecommendationFeedbackClient(dio: dio);
      const eventId = '018f3f27-0000-7000-8000-000000000202';

      final key = await api.logImpressions(
        const [
          {
            'section_id': 'local_quick_picks',
            'source': 'local',
            'seed_id': '',
            'media_id': 'local:one',
            'position': 0,
          },
        ],
        idempotencyKey: eventId,
      );

      expect(key, eventId);
      expect(rec.keys, [eventId]);
      expect(rec.paths, ['/api/v1/impressions']);
      expect(rec.bodies.single, {
        'impressions': [
          {
            'section_id': 'local_quick_picks',
            'source': 'local',
            'seed_id': '',
            'media_id': 'local:one',
            'position': 0,
          },
        ],
      });
    });

    test('does not send empty impression batches', () async {
      final api = DirectRecommendationFeedbackClient(dio: dio);

      final key = await api.logImpressions(const []);

      expect(key, isNull);
      expect(rec.paths, isEmpty);
    });
  });
}
