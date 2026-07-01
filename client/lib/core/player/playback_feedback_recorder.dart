import 'package:drift/drift.dart' show Value;

import '../api/api_client.dart';
import '../api/sunflower_api.dart';
import '../db/database.dart';

typedef ScrobbleSink = Future<String> Function({
  required String mediaId,
  required String queueId,
  required int totalPlayedMs,
  required int durationMs,
  required DateTime occurredAt,
  String? idempotencyKey,
});

abstract class LocalRecommendationRecorder {
  Future<String?> recordSongPlayback(
    Song song, {
    required String queueId,
    required DateTime occurredAt,
  });

  Future<String?> recordStreamPlayback(
    ResolvedStream stream, {
    required String queueId,
    required DateTime occurredAt,
  });

  Future<String?> recordCompletion({
    required String mediaId,
    required String queueId,
    required int totalPlayedMs,
    required int durationMs,
    required DateTime occurredAt,
    String? eventId,
  });

  Future<String?> recordPreference({
    required String mediaId,
    required bool liked,
    required DateTime occurredAt,
    String? eventId,
  });

  Future<String?> recordImpression({
    required String sectionId,
    required String source,
    required String seedId,
    required String mediaId,
    required int position,
    required DateTime occurredAt,
    String? eventId,
  });

  Future<void> markFeedbackQueued(List<String> eventIds);
}

const scrobbleFloorMs = 30000;

class PlaybackFeedbackRecorder {
  const PlaybackFeedbackRecorder({
    required this.db,
    ScrobbleSink? scrobble,
    LocalRecommendationRecorder? localRecommendations,
  })  : _scrobble = scrobble,
        _localRecommendations = localRecommendations;

  factory PlaybackFeedbackRecorder.fromBufferedApi({
    required SunflowerDatabase? db,
    BufferedApiClient? bufferedApi,
    LocalRecommendationRecorder? localRecommendations,
  }) {
    return PlaybackFeedbackRecorder(
      db: db,
      scrobble: bufferedApi?.scrobble,
      localRecommendations: localRecommendations,
    );
  }

  final SunflowerDatabase? db;
  final ScrobbleSink? _scrobble;
  final LocalRecommendationRecorder? _localRecommendations;

  Future<void> recordSong(Song song, {String queueId = ''}) {
    return _recordSong(song, queueId: queueId);
  }

  Future<void> recordStream(ResolvedStream stream, {String queueId = ''}) {
    return _recordStream(stream, queueId: queueId);
  }

  Future<void> _recordSong(Song song, {required String queueId}) async {
    final now = DateTime.now();
    await _recordDriftPlay(
      mediaId: song.mediaId,
      title: song.title,
      artistName: song.artistName,
      source: song.sourceType,
      streamUrl: null,
      durationMs: song.durationMs ?? 0,
      occurredAt: now,
    );
    try {
      await _localRecommendations?.recordSongPlayback(
        song,
        queueId: queueId,
        occurredAt: now,
      );
    } catch (_) {
      // Local core stats are advisory; keep playback independent.
    }
  }

  Future<void> _recordStream(
    ResolvedStream stream, {
    required String queueId,
  }) async {
    final now = DateTime.now();
    await _recordDriftPlay(
      mediaId: stream.mediaId,
      title: stream.title,
      artistName: stream.artists.isEmpty ? '' : stream.artists.first,
      source: stream.source,
      streamUrl: stream.source == 'local' || _isFileUrl(stream.streamUrl)
          ? stream.streamUrl
          : null,
      durationMs: stream.durationMs,
      occurredAt: now,
    );
    try {
      await _localRecommendations?.recordStreamPlayback(
        stream,
        queueId: queueId,
        occurredAt: now,
      );
    } catch (_) {
      // Local core stats are advisory; keep playback independent.
    }
  }

  Future<void> recordQueueItem(
    QueueItem item, {
    required String streamUrl,
    required String source,
    String queueId = '',
  }) {
    return _recordStream(
      ResolvedStream(
        mediaId: item.mediaId,
        source: source,
        streamUrl: streamUrl,
        title: item.title,
        artists: item.artists,
        durationMs: item.durationMs,
      ),
      queueId: queueId,
    );
  }

  bool _isFileUrl(String url) => Uri.tryParse(url)?.scheme == 'file';

  Future<void> scrobble({
    required String mediaId,
    required String queueId,
    required int totalPlayedMs,
    required int durationMs,
  }) async {
    final occurredAt = DateTime.now();
    final sink = scrobbleSink;
    if (totalPlayedMs <= 0) return;
    String? localEventId;
    try {
      localEventId = await _localRecommendations?.recordCompletion(
        mediaId: mediaId,
        queueId: queueId,
        totalPlayedMs: totalPlayedMs,
        durationMs: durationMs,
        occurredAt: occurredAt,
      );
    } catch (_) {
      // Local core stats are advisory; keep playback independent.
    }

    if (sink != null) {
      try {
        await sink(
          mediaId: mediaId,
          queueId: queueId,
          totalPlayedMs: totalPlayedMs,
          durationMs: durationMs,
          occurredAt: occurredAt,
          idempotencyKey: localEventId,
        );
      } catch (_) {
        // Replay enqueue is advisory; a failed remote feedback path must not
        // prevent local recommendations from updating.
      }
    }
  }

  Future<void> _recordDriftPlay({
    required String mediaId,
    required String title,
    required String artistName,
    required String source,
    required String? streamUrl,
    required int durationMs,
    required DateTime occurredAt,
  }) async {
    final database = db;
    if (database != null) {
      try {
        await database.recordPlay(
          RecentPlaysCompanion.insert(
            mediaId: mediaId,
            title: Value(title),
            artistName: Value(artistName),
            source: Value(source),
            streamUrl: Value(streamUrl),
            durationMs: Value(durationMs),
            lastPlayedAt: Value(occurredAt),
          ),
        );
      } catch (_) {
        // Local play history is advisory; keep remote feedback independent.
      }
    }
  }

  ScrobbleSink? get scrobbleSink => _scrobble;
}

class PlaybackScrobbleGate {
  PlaybackScrobbleGate({
    required this.mediaId,
    required this.queueId,
    required this.durationMs,
  });

  final String mediaId;
  final String queueId;
  final int durationMs;
  bool _sent = false;

  int? qualify({
    required Duration position,
    required bool playing,
    bool completed = false,
  }) {
    if (_sent || (!playing && !completed)) return null;
    final playedMs =
        completed && durationMs > 0 ? durationMs : position.inMilliseconds;
    if (playedMs < scrobbleThresholdMs(durationMs)) return null;
    _sent = true;
    return playedMs;
  }
}

int scrobbleThresholdMs(int durationMs) {
  if (durationMs <= 0) return scrobbleFloorMs;
  final half = (durationMs / 2).ceil();
  return half < scrobbleFloorMs ? half : scrobbleFloorMs;
}
