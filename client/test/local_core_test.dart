import 'package:dio/dio.dart';
import 'package:drift/native.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sunflower/core/api/api_client.dart';
import 'package:sunflower/core/api/sunflower_api.dart';
import 'package:sunflower/core/bridge/api.dart' as bridge;
import 'package:sunflower/core/bridge/frb_generated.dart';
import 'package:sunflower/core/db/database.dart';
import 'package:sunflower/core/recommendations/local_core.dart';
import 'package:sunflower/core/sync/replay_buffer.dart';

void main() {
  late _FakeRustApi api;

  setUpAll(() {
    api = _FakeRustApi();
    RustLib.initMock(api: api);
  });

  setUp(() {
    api.reset();
  });

  tearDownAll(RustLib.dispose);

  test('records playback starts locally without immediate remote queue',
      () async {
    final recorder = BridgeLocalRecommendationRecorder(
      const bridge.CoreHandle(sqlitePath: 'memory'),
    );

    final eventId = await recorder.recordSongPlayback(
      const Song(
        mediaId: 'local:one',
        sourceType: 'local',
        title: 'One',
        artistName: 'Artist',
        albumTitle: '',
        hasArt: false,
        durationMs: 42000,
      ),
      queueId: '018f3f27-0000-7000-8000-000000000010',
      occurredAt: DateTime.utc(2026, 7, 1, 1, 2, 3),
    );

    expect(eventId, _FakeRustApi.eventId);
    expect(api.upsertedSongs.single.mediaId, 'local:one');
    expect(api.appendedEvents.single.eventId, _FakeRustApi.eventId);
    expect(api.appendedEvents.single.kind,
        bridge.RecommendationEventKindDto.playStarted);
    expect(api.markedSyncedBatches, isEmpty);

    await recorder.markFeedbackQueued([eventId!]);

    expect(api.markedSyncedBatches, [
      [_FakeRustApi.eventId],
    ]);
  });

  test('drain marks playback starts handled without remote replay', () async {
    final recorder = BridgeLocalRecommendationRecorder(
      const bridge.CoreHandle(sqlitePath: 'memory'),
    );
    final db = SunflowerDatabase.forTesting(NativeDatabase.memory());
    addTearDown(db.close);
    final dio = Dio(BaseOptions(baseUrl: 'http://test'));
    final requests = _RecordingInterceptor();
    dio.interceptors.add(requests);
    var clock = 100;
    final buffered = BufferedApiClient(
      ReplayBuffer(
        dio: dio,
        db: db,
        nowMs: () => clock++,
      ),
    );
    api.appendedEvents.add(
      bridge.RecommendationEventDto(
        eventId: _FakeRustApi.eventId,
        clientClock: 1,
        occurredAtMs: DateTime.utc(2026, 7, 1, 1, 2, 3).millisecondsSinceEpoch,
        kind: bridge.RecommendationEventKindDto.playStarted,
        mediaId: 'local:one',
        queueId: '018f3f27-0000-7000-8000-000000000010',
        recommenderSource: bridge.RecommendationSourceDto.local,
        contextJson: '{}',
        payloadJson: '{"duration_ms":42000}',
      ),
    );

    final queued = await recorder.drainFeedbackToServer(buffered);

    expect(queued, 1);
    expect(requests.keys, isEmpty);
    expect(requests.paths, isEmpty);
    expect(requests.bodies, isEmpty);
    expect(api.markedSyncedBatches, [
      [_FakeRustApi.eventId],
    ]);
    expect(await db.pendingCount(), 0);
  });

  test('drains completed local playback feedback into feedback client',
      () async {
    final recorder = BridgeLocalRecommendationRecorder(
      const bridge.CoreHandle(sqlitePath: 'memory'),
    );
    final db = SunflowerDatabase.forTesting(NativeDatabase.memory());
    addTearDown(db.close);
    final dio = Dio(BaseOptions(baseUrl: 'http://test'));
    final requests = _RecordingInterceptor();
    dio.interceptors.add(requests);
    var clock = 100;
    final buffered = BufferedApiClient(
      ReplayBuffer(
        dio: dio,
        db: db,
        nowMs: () => clock++,
      ),
    );
    api.appendedEvents.add(
      bridge.RecommendationEventDto(
        eventId: _FakeRustApi.eventId,
        clientClock: 1,
        occurredAtMs: DateTime.utc(2026, 7, 1, 1, 2, 3).millisecondsSinceEpoch,
        kind: bridge.RecommendationEventKindDto.playCompleted,
        mediaId: 'local:one',
        queueId: '018f3f27-0000-7000-8000-000000000010',
        recommenderSource: bridge.RecommendationSourceDto.local,
        contextJson: '{}',
        payloadJson: '{"total_played_ms":30000,"duration_ms":42000}',
      ),
    );

    final queued = await recorder.drainFeedbackToServer(buffered);

    expect(queued, 1);
    expect(requests.keys, [_FakeRustApi.eventId]);
    expect(requests.paths, ['/api/v1/events']);
    expect(requests.bodies.single, {
      'events': [
        {
          'event_id': _FakeRustApi.eventId,
          'kind': 'play',
          'media_id': 'local:one',
          'queue_id': '018f3f27-0000-7000-8000-000000000010',
          'total_played_ms': 30000,
          'duration_ms': 42000,
          'occurred_at': '2026-07-01T01:02:03.000Z',
        },
      ],
    });
    expect(api.markedSyncedBatches, [
      [_FakeRustApi.eventId],
    ]);
    expect(await db.pendingCount(), 0);
  });

  test('drains local skip preference and impression feedback in order',
      () async {
    final recorder = BridgeLocalRecommendationRecorder(
      const bridge.CoreHandle(sqlitePath: 'memory'),
    );
    final db = SunflowerDatabase.forTesting(NativeDatabase.memory());
    addTearDown(db.close);
    final dio = Dio(BaseOptions(baseUrl: 'http://test'));
    final requests = _RecordingInterceptor();
    dio.interceptors.add(requests);
    var clock = 100;
    final buffered = BufferedApiClient(
      ReplayBuffer(
        dio: dio,
        db: db,
        nowMs: () => clock++,
      ),
    );
    const skipId = '018f3f27-0000-7000-8000-000000000002';
    const likeId = '018f3f27-0000-7000-8000-000000000003';
    const impressionId = '018f3f27-0000-7000-8000-000000000004';
    api.appendedEvents.addAll([
      _event(
        eventId: skipId,
        clientClock: 1,
        kind: bridge.RecommendationEventKindDto.skipped,
        mediaId: 'yt:skip',
        queueId: '018f3f27-0000-7000-8000-000000000010',
        payloadJson:
            '{"total_played_ms":12000,"duration_ms":42000,"reason":"user_skip"}',
      ),
      _event(
        eventId: likeId,
        clientClock: 2,
        kind: bridge.RecommendationEventKindDto.liked,
        mediaId: 'yt:liked',
        payloadJson: '{"liked":true}',
      ),
      _event(
        eventId: impressionId,
        clientClock: 3,
        kind: bridge.RecommendationEventKindDto.impression,
        mediaId: 'yt:shown',
        payloadJson:
            '{"section_id":"daily","source":"yt","seed_id":"seed","position":7}',
      ),
    ]);

    final queued = await recorder.drainFeedbackToServer(buffered);

    expect(queued, 3);
    expect(requests.keys, [skipId, likeId, impressionId]);
    expect(requests.paths, [
      '/api/v1/events',
      '/api/v1/likes',
      '/api/v1/impressions',
    ]);
    expect(requests.bodies[0], {
      'events': [
        {
          'event_id': skipId,
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
    expect(requests.bodies[1], {
      'media_id': 'yt:liked',
      'liked': true,
      'occurred_at': '2026-07-01T01:02:03.000Z',
    });
    expect(requests.bodies[2], {
      'impressions': [
        {
          'section_id': 'daily',
          'source': 'yt',
          'seed_id': 'seed',
          'media_id': 'yt:shown',
          'position': 7,
        },
      ],
    });
    expect(api.markedSyncedBatches, [
      [skipId, likeId, impressionId],
    ]);
    expect(await db.pendingCount(), 0);
  });

  test('drain preserves failed and later local feedback for retry', () async {
    final recorder = BridgeLocalRecommendationRecorder(
      const bridge.CoreHandle(sqlitePath: 'memory'),
    );
    const completedId = '018f3f27-0000-7000-8000-000000000005';
    const likeId = '018f3f27-0000-7000-8000-000000000006';
    const impressionId = '018f3f27-0000-7000-8000-000000000007';
    api.appendedEvents.addAll([
      _event(
        eventId: completedId,
        clientClock: 1,
        kind: bridge.RecommendationEventKindDto.playCompleted,
        mediaId: 'local:done',
        payloadJson: '{"total_played_ms":30000,"duration_ms":42000}',
      ),
      _event(
        eventId: likeId,
        clientClock: 2,
        kind: bridge.RecommendationEventKindDto.liked,
        mediaId: 'local:liked',
        payloadJson: '{"liked":true}',
      ),
      _event(
        eventId: impressionId,
        clientClock: 3,
        kind: bridge.RecommendationEventKindDto.impression,
        mediaId: 'local:shown',
        payloadJson:
            '{"section_id":"local_quick_picks","source":"local","seed_id":"","position":0}',
      ),
    ]);
    final feedback = _FailingFeedbackClient(failOnCall: 2);

    final queued = await recorder.drainFeedbackToServer(feedback);

    expect(queued, 1);
    expect(feedback.calls, [
      _FeedbackCall('/api/v1/events', completedId),
      _FeedbackCall('/api/v1/likes', likeId),
    ]);
    expect(api.markedSyncedBatches, [
      [completedId],
    ]);
  });
}

bridge.RecommendationEventDto _event({
  required String eventId,
  required int clientClock,
  required bridge.RecommendationEventKindDto kind,
  required String mediaId,
  String? queueId,
  String payloadJson = '{}',
}) {
  return bridge.RecommendationEventDto(
    eventId: eventId,
    clientClock: clientClock,
    occurredAtMs: DateTime.utc(2026, 7, 1, 1, 2, 3).millisecondsSinceEpoch,
    kind: kind,
    mediaId: mediaId,
    queueId: queueId,
    recommenderSource: bridge.RecommendationSourceDto.local,
    contextJson: '{}',
    payloadJson: payloadJson,
  );
}

class _RecordingInterceptor extends Interceptor {
  final keys = <String>[];
  final paths = <String>[];
  final bodies = <Map<String, dynamic>?>[];

  @override
  void onRequest(RequestOptions options, RequestInterceptorHandler handler) {
    keys.add(options.headers['Idempotency-Key'] as String);
    paths.add(options.path);
    final data = options.data;
    bodies.add(data is Map ? Map<String, dynamic>.from(data) : null);
    handler.resolve(Response(requestOptions: options, statusCode: 200));
  }
}

class _FeedbackCall {
  const _FeedbackCall(this.path, this.idempotencyKey);

  final String path;
  final String? idempotencyKey;

  @override
  bool operator ==(Object other) {
    return other is _FeedbackCall &&
        other.path == path &&
        other.idempotencyKey == idempotencyKey;
  }

  @override
  int get hashCode => Object.hash(path, idempotencyKey);

  @override
  String toString() => '_FeedbackCall($path, $idempotencyKey)';
}

class _FailingFeedbackClient implements RecommendationFeedbackClient {
  _FailingFeedbackClient({required this.failOnCall});

  final int failOnCall;
  final calls = <_FeedbackCall>[];

  Future<T> _record<T>(
    String path,
    String? idempotencyKey,
    T value,
  ) async {
    calls.add(_FeedbackCall(path, idempotencyKey));
    if (calls.length == failOnCall) {
      throw DioException(
        requestOptions: RequestOptions(path: path),
        response: Response<void>(
          requestOptions: RequestOptions(path: path),
          statusCode: 503,
        ),
      );
    }
    return value;
  }

  @override
  Future<String> like(
    String mediaId, {
    required bool liked,
    DateTime? occurredAt,
    String? idempotencyKey,
  }) {
    return _record('/api/v1/likes', idempotencyKey, idempotencyKey ?? 'like');
  }

  @override
  Future<String?> logImpressions(
    List<Map<String, dynamic>> impressions, {
    String? idempotencyKey,
  }) {
    return _record('/api/v1/impressions', idempotencyKey, idempotencyKey);
  }

  @override
  Future<String> playbackEvent({
    required String kind,
    required String mediaId,
    required String queueId,
    required DateTime occurredAt,
    int totalPlayedMs = 0,
    int durationMs = 0,
    String reason = '',
    String? idempotencyKey,
  }) {
    return _record('/api/v1/events', idempotencyKey, idempotencyKey ?? 'event');
  }
}

class _FakeRustApi extends RustLibApi {
  static const eventId = '018f3f27-0000-7000-8000-000000000001';

  final upsertedSongs = <bridge.SongDto>[];
  final appendedEvents = <bridge.RecommendationEventDto>[];
  final markedSyncedBatches = <List<String>>[];

  void reset() {
    upsertedSongs.clear();
    appendedEvents.clear();
    markedSyncedBatches.clear();
  }

  @override
  Future<void> crateApiAppendRecommendationEvent({
    required bridge.CoreHandle handle,
    required bridge.RecommendationEventDto event,
  }) async {
    appendedEvents.add(event);
  }

  @override
  Future<bridge.LocalStatsSnapshotDto> crateApiEmptyStatsSnapshot() async {
    return const bridge.LocalStatsSnapshotDto(
      generatedAtMs: 0,
      tracks: [],
      recentMediaIds: [],
      recentArtistNames: [],
    );
  }

  @override
  Future<bridge.RecommendationSnapshotDto?>
      crateApiLatestRecommendationSnapshot({
    required bridge.CoreHandle handle,
  }) async {
    return null;
  }

  @override
  Future<List<bridge.SongDto>> crateApiListLocalSongs({
    required bridge.CoreHandle handle,
    required int limit,
    required int offset,
  }) async {
    return upsertedSongs.take(limit).toList();
  }

  @override
  Future<bridge.RecommendationCandidateDto> crateApiLocalCandidate({
    required String mediaId,
    required String title,
  }) async {
    return bridge.RecommendationCandidateDto(
      mediaId: mediaId,
      title: title,
      artists: const [],
      durationMs: 0,
      source: bridge.RecommendationSourceDto.local,
      remoteScore: 0,
    );
  }

  @override
  Future<bridge.LocalStatsSnapshotDto> crateApiLocalStatsSnapshot({
    required bridge.CoreHandle handle,
    required int recentLimit,
  }) async {
    return const bridge.LocalStatsSnapshotDto(
      generatedAtMs: 0,
      tracks: [],
      recentMediaIds: [],
      recentArtistNames: [],
    );
  }

  @override
  Future<void> crateApiMarkRecommendationEventsSynced({
    required bridge.CoreHandle handle,
    required List<String> eventIds,
  }) async {
    markedSyncedBatches.add([...eventIds]);
  }

  @override
  Future<String> crateApiNewEventId() async => eventId;

  @override
  Future<bridge.CoreHandle> crateApiOpenCore({
    required bridge.CoreConfig config,
  }) async {
    return bridge.CoreHandle(
      sqlitePath: config.sqlitePath,
      recommendationServerUrl: config.recommendationServerUrl,
    );
  }

  @override
  Future<void> crateApiPutRecommendationSnapshot({
    required bridge.CoreHandle handle,
    required bridge.RecommendationSnapshotDto snapshot,
  }) async {}

  @override
  Future<List<bridge.RecommendationCandidateDto>> crateApiRankLocalCandidates({
    required List<bridge.RecommendationCandidateDto> candidates,
    required bridge.LocalStatsSnapshotDto stats,
    required int limit,
  }) async {
    return candidates.take(limit).toList();
  }

  @override
  Future<List<bridge.RecommendationCandidateDto>>
      crateApiRankLocalFromSnapshot({
    required bridge.CoreHandle handle,
    required bridge.LocalStatsSnapshotDto stats,
    required int limit,
  }) async {
    return const [];
  }

  @override
  Future<bridge.SongDto> crateApiSongFromLocalFile({
    required String mediaId,
    required String title,
    required String path,
  }) async {
    return bridge.SongDto(
      mediaId: mediaId,
      sourceType: 'local',
      title: title,
      artists: const [],
      explicit: false,
      videoOnly: false,
      available: true,
      localPath: path,
    );
  }

  @override
  Future<List<bridge.RecommendationEventDto>>
      crateApiUnsyncedRecommendationEvents({
    required bridge.CoreHandle handle,
    required int limit,
  }) async {
    return appendedEvents.take(limit).toList();
  }

  @override
  Future<void> crateApiUpsertLocalSong({
    required bridge.CoreHandle handle,
    required bridge.SongDto song,
  }) async {
    upsertedSongs.add(song);
  }
}
