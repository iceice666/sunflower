import 'package:drift/native.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sunflower/core/api/sunflower_api.dart';
import 'package:sunflower/core/db/database.dart';
import 'package:sunflower/core/player/playback_feedback_recorder.dart';

void main() {
  late SunflowerDatabase db;
  late List<_ScrobbleCall> scrobbles;
  late _RecordingLocalRecorder local;
  late PlaybackFeedbackRecorder recorder;

  setUp(() {
    db = SunflowerDatabase.forTesting(NativeDatabase.memory());
    scrobbles = [];
    local = _RecordingLocalRecorder();
    recorder = PlaybackFeedbackRecorder(
      db: db,
      localRecommendations: local,
      scrobble: ({
        required mediaId,
        required queueId,
        required totalPlayedMs,
        required durationMs,
        required occurredAt,
        idempotencyKey,
      }) async {
        scrobbles.add(
          _ScrobbleCall(
            mediaId: mediaId,
            queueId: queueId,
            totalPlayedMs: totalPlayedMs,
            durationMs: durationMs,
            idempotencyKey: idempotencyKey,
          ),
        );
        return idempotencyKey ?? '018f3f27-0000-7000-8000-000000000010';
      },
    );
  });

  tearDown(() async {
    await db.close();
  });

  test('records direct song playback locally without immediate scrobble',
      () async {
    await recorder.recordSong(
      const Song(
        mediaId: 'local:one',
        sourceType: 'local',
        title: 'One',
        artistName: 'A',
        albumTitle: '',
        hasArt: false,
        durationMs: 40000,
      ),
    );

    final rows = await db.recentPlays_();
    expect(rows.single.mediaId, 'local:one');
    expect(rows.single.playCount, 1);
    expect(rows.single.durationMs, 40000);
    expect(scrobbles, isEmpty);
    expect(local.songPlays.single.song.mediaId, 'local:one');
  });

  test('queues server feedback when the scrobble gate qualifies', () async {
    final gate = PlaybackScrobbleGate(
      mediaId: 'local:one',
      queueId: '',
      durationMs: 40000,
    );

    expect(
      gate.qualify(
        position: const Duration(milliseconds: 19999),
        playing: true,
      ),
      isNull,
    );

    final totalPlayedMs = gate.qualify(
      position: const Duration(milliseconds: 20000),
      playing: true,
    );
    expect(totalPlayedMs, 20000);

    await recorder.scrobble(
      mediaId: gate.mediaId,
      queueId: gate.queueId,
      totalPlayedMs: totalPlayedMs!,
      durationMs: gate.durationMs,
    );

    expect(scrobbles.single.mediaId, 'local:one');
    expect(scrobbles.single.totalPlayedMs, 20000);
    expect(scrobbles.single.idempotencyKey,
        '018f3f27-0000-7000-8000-000000000001');
    expect(local.completions.single.mediaId, 'local:one');
    expect(local.completions.single.totalPlayedMs, 20000);
    expect(local.queuedEventIds, isEmpty);
    expect(
      gate.qualify(
        position: const Duration(milliseconds: 40000),
        playing: true,
      ),
      isNull,
    );
  });

  test('scrobble gate does not qualify while paused before completion', () {
    final gate = PlaybackScrobbleGate(
      mediaId: 'local:one',
      queueId: '',
      durationMs: 240000,
    );

    expect(
      gate.qualify(
        position: const Duration(milliseconds: 30000),
        playing: false,
      ),
      isNull,
    );
    expect(
      gate.qualify(
        position: const Duration(milliseconds: 30000),
        playing: true,
      ),
      30000,
    );
  });

  test('does not persist transient remote stream URLs into local history',
      () async {
    await recorder.recordStream(
      const ResolvedStream(
        mediaId: 'yt:remote',
        source: 'youtube',
        streamUrl: 'https://googlevideo.example/transient',
        title: 'Remote',
        artists: ['B'],
        durationMs: 240000,
      ),
      queueId: '018f3f27-0000-7000-8000-000000000001',
    );

    final row = (await db.recentPlays_()).single;
    expect(row.mediaId, 'yt:remote');
    expect(row.streamUrl, isNull);
    expect(scrobbles, isEmpty);
    expect(local.streamPlays.single.stream.mediaId, 'yt:remote');
  });

  test('records local completion even without a remote scrobble sink',
      () async {
    final recorder = PlaybackFeedbackRecorder(
      db: db,
      localRecommendations: local,
    );

    await recorder.scrobble(
      mediaId: 'local:offline',
      queueId: '',
      totalPlayedMs: 30000,
      durationMs: 60000,
    );

    expect(scrobbles, isEmpty);
    expect(local.completions.single.mediaId, 'local:offline');
    expect(local.completions.single.durationMs, 60000);
  });

  test('scrobble threshold mirrors the server window', () {
    expect(scrobbleThresholdMs(240000), 30000);
    expect(scrobbleThresholdMs(40000), 20000);
    expect(scrobbleThresholdMs(0), 30000);

    final completedShortTrack = PlaybackScrobbleGate(
      mediaId: 'local:short',
      queueId: '',
      durationMs: 10000,
    );
    expect(
      completedShortTrack.qualify(
        position: Duration.zero,
        playing: false,
        completed: true,
      ),
      10000,
    );
  });
}

class _ScrobbleCall {
  const _ScrobbleCall({
    required this.mediaId,
    required this.queueId,
    required this.totalPlayedMs,
    required this.durationMs,
    required this.idempotencyKey,
  });

  final String mediaId;
  final String queueId;
  final int totalPlayedMs;
  final int durationMs;
  final String? idempotencyKey;
}

class _RecordingLocalRecorder implements LocalRecommendationRecorder {
  final songPlays = <_SongPlay>[];
  final streamPlays = <_StreamPlay>[];
  final completions = <_Completion>[];
  final preferences = <_Preference>[];
  final impressions = <_Impression>[];
  final queuedEventIds = <String>[];
  int _next = 0;

  @override
  Future<String?> recordSongPlayback(
    Song song, {
    required String queueId,
    required DateTime occurredAt,
  }) async {
    songPlays.add(_SongPlay(song, queueId, occurredAt));
    return _eventId();
  }

  @override
  Future<String?> recordStreamPlayback(
    ResolvedStream stream, {
    required String queueId,
    required DateTime occurredAt,
  }) async {
    streamPlays.add(_StreamPlay(stream, queueId, occurredAt));
    return _eventId();
  }

  @override
  Future<String?> recordCompletion({
    required String mediaId,
    required String queueId,
    required int totalPlayedMs,
    required int durationMs,
    required DateTime occurredAt,
    String? eventId,
  }) async {
    completions.add(
      _Completion(
        mediaId: mediaId,
        queueId: queueId,
        totalPlayedMs: totalPlayedMs,
        durationMs: durationMs,
        occurredAt: occurredAt,
      ),
    );
    return eventId ?? _eventId();
  }

  @override
  Future<String?> recordPreference({
    required String mediaId,
    required bool liked,
    required DateTime occurredAt,
    String? eventId,
  }) async {
    preferences.add(_Preference(mediaId, liked, occurredAt));
    return eventId ?? _eventId();
  }

  @override
  Future<String?> recordImpression({
    required String sectionId,
    required String source,
    required String seedId,
    required String mediaId,
    required int position,
    required DateTime occurredAt,
    String? eventId,
  }) async {
    impressions.add(
      _Impression(sectionId, source, seedId, mediaId, position, occurredAt),
    );
    return eventId ?? _eventId();
  }

  @override
  Future<void> markFeedbackQueued(List<String> eventIds) async {
    queuedEventIds.addAll(eventIds);
  }

  String _eventId() {
    _next += 1;
    return '018f3f27-0000-7000-8000-${_next.toString().padLeft(12, '0')}';
  }
}

class _SongPlay {
  const _SongPlay(this.song, this.queueId, this.occurredAt);

  final Song song;
  final String queueId;
  final DateTime occurredAt;
}

class _StreamPlay {
  const _StreamPlay(this.stream, this.queueId, this.occurredAt);

  final ResolvedStream stream;
  final String queueId;
  final DateTime occurredAt;
}

class _Completion {
  const _Completion({
    required this.mediaId,
    required this.queueId,
    required this.totalPlayedMs,
    required this.durationMs,
    required this.occurredAt,
  });

  final String mediaId;
  final String queueId;
  final int totalPlayedMs;
  final int durationMs;
  final DateTime occurredAt;
}

class _Preference {
  const _Preference(this.mediaId, this.liked, this.occurredAt);

  final String mediaId;
  final bool liked;
  final DateTime occurredAt;
}

class _Impression {
  const _Impression(
    this.sectionId,
    this.source,
    this.seedId,
    this.mediaId,
    this.position,
    this.occurredAt,
  );

  final String sectionId;
  final String source;
  final String seedId;
  final String mediaId;
  final int position;
  final DateTime occurredAt;
}
