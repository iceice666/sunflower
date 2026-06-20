import 'package:dio/dio.dart';
import 'package:drift/native.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sunflower/core/db/database.dart';
import 'package:sunflower/core/sync/eviction.dart';
import 'package:sunflower/core/sync/replay_buffer.dart';
import 'package:sunflower/core/sync/retry_policy.dart';

// Records every request the buffer replays so we can assert order + idempotency.
class _RecordingInterceptor extends Interceptor {
  final List<String> keys = [];
  final List<String> paths = [];
  int failFirstN = 0;
  _RecordingInterceptor();

  @override
  void onRequest(RequestOptions options, RequestInterceptorHandler handler) {
    keys.add(options.headers['Idempotency-Key'] as String);
    paths.add(options.path);
    if (failFirstN > 0) {
      failFirstN--;
      handler.reject(DioException(
        requestOptions: options,
        error: 'simulated failure',
      ));
      return;
    }
    handler.resolve(Response(requestOptions: options, statusCode: 200));
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
          greaterThan(Eviction.priorityFor('event')));
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
  });
}
