import 'dart:async';

import 'package:drift/drift.dart' show Value;
import 'package:drift/native.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sunflower/core/api/api_client.dart';
import 'package:sunflower/core/api/sunflower_api.dart';
import 'package:sunflower/core/db/database.dart';
import 'package:sunflower/core/downloads/download_manager.dart';
import 'package:sunflower/core/downloads/download_worker.dart';
import 'package:sunflower/core/downloads/isolate_runner.dart';
import 'package:sunflower/core/downloads/storage.dart';

void main() {
  late SunflowerDatabase db;
  late _RecordingApi api;
  late _RecordingBufferedApi bufferedApi;
  late _FakeRunner runner;
  late DownloadManager manager;

  setUp(() {
    db = SunflowerDatabase.forTesting(NativeDatabase.memory());
    api = _RecordingApi();
    bufferedApi = _RecordingBufferedApi();
    runner = _FakeRunner();
    manager = DownloadManager(
      api: api,
      db: db,
      deviceId: '018f3f27-0000-7000-8000-000000000001',
      bufferedApi: bufferedApi,
      runner: runner,
      storage: _FakeStorage(),
    );
  });

  tearDown(() async {
    await manager.dispose();
    await db.close();
  });

  test('completed downloads register through the replay buffer first',
      () async {
    await manager.start();
    await manager.enqueueTrack(
      mediaId: 'yt:track',
      title: 'Track',
      streamUrl: 'https://example.invalid/audio',
    );

    runner.emit(const DownloadComplete('yt:track', '/tmp/yt_track.audio', 123));
    await _flushAsync();

    expect(api.registerCalls, isEmpty);
    expect(bufferedApi.registerCalls.single, (
      deviceId: '018f3f27-0000-7000-8000-000000000001',
      mediaId: 'yt:track',
      localPath: '/tmp/yt_track.audio',
      bytes: 123,
    ));
    expect(await db.downloadedTrack('yt:track'), isNotNull);
  });

  test('download removal unregisters through the replay buffer first',
      () async {
    await db.completeDownload(
      DownloadedTracksCompanion.insert(
        mediaId: 'yt:track',
        localPath: '/tmp/yt_track.audio',
        bytes: const Value(123),
      ),
    );

    await manager.remove('yt:track');

    expect(api.deleteCalls, isEmpty);
    expect(bufferedApi.removeCalls.single, (
      deviceId: '018f3f27-0000-7000-8000-000000000001',
      mediaId: 'yt:track',
    ));
    expect(await db.downloadedTrack('yt:track'), isNull);
  });

  test('playlist downloads resolve yt tracks and stream local tracks',
      () async {
    api.playlist = const Playlist(
      id: 'playlist-1',
      title: 'Playlist',
      version: 1,
      items: [
        HomeItem(mediaId: 'yt:remote', title: 'Remote', source: 'yt'),
        HomeItem(mediaId: 'local:one', title: 'Local', source: 'local'),
      ],
    );
    api.resolvedStreams['yt:remote'] = const ResolvedStream(
      mediaId: 'yt:remote',
      source: 'youtube',
      streamUrl: 'https://stream.example/remote.audio',
    );
    await manager.start();

    await manager.enqueuePlaylist('playlist-1');

    expect(api.playlistRequests, ['playlist-1']);
    expect(api.resolveCalls, ['yt:remote']);
    expect(runner.requests.map((request) => request.mediaId), [
      'yt:remote',
      'local:one',
    ]);
    expect(runner.requests.map((request) => request.url), [
      'https://stream.example/remote.audio',
      'http://test.invalid/api/v1/library/songs/local%3Aone/stream',
    ]);
  });
}

Future<void> _flushAsync() async {
  await Future<void>.delayed(Duration.zero);
  await Future<void>.delayed(Duration.zero);
}

class _RecordingApi extends SunflowerApi {
  _RecordingApi() : super(baseUrl: 'http://test.invalid', token: 'token');

  final registerCalls =
      <({String deviceId, String mediaId, String localPath, int bytes})>[];
  final deleteCalls = <({String deviceId, String mediaId})>[];
  final playlistRequests = <String>[];
  final resolveCalls = <String>[];
  final resolvedStreams = <String, ResolvedStream>{};
  Playlist playlist = const Playlist(
    id: 'playlist-1',
    title: 'Playlist',
    version: 1,
    items: [],
  );

  @override
  Map<String, String> get authHeaders => const {};

  @override
  Future<Playlist> getPlaylist(String id) async {
    playlistRequests.add(id);
    return playlist;
  }

  @override
  Future<ResolvedStream> resolveStream(
    String mediaId, {
    bool proxy = false,
    String? audioQuality,
    String? reason,
  }) async {
    resolveCalls.add(mediaId);
    return resolvedStreams[mediaId] ??
        ResolvedStream(
          mediaId: mediaId,
          source: 'youtube',
          streamUrl: 'https://stream.example/$mediaId.audio',
        );
  }

  @override
  Future<void> registerDownload({
    required String deviceId,
    required String mediaId,
    required String localPath,
    required int bytes,
  }) async {
    registerCalls.add((
      deviceId: deviceId,
      mediaId: mediaId,
      localPath: localPath,
      bytes: bytes,
    ));
  }

  @override
  Future<void> deleteDownload(String deviceId, String mediaId) async {
    deleteCalls.add((deviceId: deviceId, mediaId: mediaId));
  }
}

class _RecordingBufferedApi implements BufferedApiClient {
  final registerCalls =
      <({String deviceId, String mediaId, String localPath, int bytes})>[];
  final removeCalls = <({String deviceId, String mediaId})>[];

  @override
  int get overflowDrops => 0;

  @override
  Stream<int> watchPendingCount() => Stream.value(0);

  @override
  Future<void> retryNow() async {}

  @override
  Future<String> like(
    String mediaId, {
    required bool liked,
    DateTime? occurredAt,
    String? idempotencyKey,
  }) async =>
      idempotencyKey ?? '018f3f27-0000-7000-8000-000000000101';

  @override
  Future<String> scrobble({
    required String mediaId,
    required String queueId,
    required int totalPlayedMs,
    required int durationMs,
    required DateTime occurredAt,
    String? idempotencyKey,
  }) async =>
      idempotencyKey ?? '018f3f27-0000-7000-8000-000000000102';

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
  }) async =>
      idempotencyKey ?? '018f3f27-0000-7000-8000-000000000102';

  @override
  Future<String?> logImpressions(
    List<Map<String, dynamic>> impressions, {
    String? idempotencyKey,
  }) async =>
      idempotencyKey ?? '018f3f27-0000-7000-8000-000000000103';

  @override
  Future<void> addPlaylistItem(String playlistId, String mediaId) async {}

  @override
  Future<String> createPlaylist(String title, {String? idempotencyKey}) async =>
      idempotencyKey ?? '018f3f27-0000-7000-8000-000000000104';

  @override
  Future<void> removePlaylistItem(String playlistId, String mediaId) async {}

  @override
  Future<void> renamePlaylist(String playlistId, String title) async {}

  @override
  Future<void> deletePlaylist(String playlistId) async {}

  @override
  Future<void> registerDownload({
    required String deviceId,
    required String mediaId,
    required String localPath,
    required int bytes,
  }) async {
    registerCalls.add((
      deviceId: deviceId,
      mediaId: mediaId,
      localPath: localPath,
      bytes: bytes,
    ));
  }

  @override
  Future<void> removeDownload(String deviceId, String mediaId) async {
    removeCalls.add((deviceId: deviceId, mediaId: mediaId));
  }
}

class _FakeRunner extends IsolateRunner {
  final _events = StreamController<DownloadEvent>.broadcast();
  final requests = <DownloadRequest>[];

  @override
  Stream<DownloadEvent> get events => _events.stream;

  @override
  Future<void> start() async {}

  @override
  void enqueue(DownloadRequest req) {
    requests.add(req);
  }

  void emit(DownloadEvent event) {
    _events.add(event);
  }

  @override
  Future<void> dispose() => _events.close();
}

class _FakeStorage extends DownloadStorage {
  final deleted = <String>[];

  @override
  Future<String> pathFor(String mediaId, {String extension = 'audio'}) async {
    return '/tmp/${mediaId.replaceAll(':', '_')}.$extension';
  }

  @override
  Future<String> partialPathFor(
    String mediaId, {
    String extension = 'audio',
  }) async {
    return '${await pathFor(mediaId, extension: extension)}.part';
  }

  @override
  Future<void> delete(String mediaId, {String extension = 'audio'}) async {
    deleted.add(mediaId);
  }

  @override
  Future<void> deletePartial(
    String mediaId, {
    String extension = 'audio',
  }) async {}
}
