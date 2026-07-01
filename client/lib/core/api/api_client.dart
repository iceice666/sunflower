import 'package:dio/dio.dart';

import '../sync/replay_buffer.dart';
import '../sync/idempotency_key.dart';

abstract interface class RecommendationFeedbackClient {
  Future<String> like(
    String mediaId, {
    required bool liked,
    DateTime? occurredAt,
    String? idempotencyKey,
  });

  Future<String> playbackEvent({
    required String kind,
    required String mediaId,
    required String queueId,
    required DateTime occurredAt,
    int totalPlayedMs = 0,
    int durationMs = 0,
    String reason = '',
    String? idempotencyKey,
  });

  Future<String?> logImpressions(
    List<Map<String, dynamic>> impressions, {
    String? idempotencyKey,
  });
}

/// Mutation-routing facade (M7): every mutating call goes through the
/// [ReplayBuffer] first so it is durably queued and replayed in client-clock
/// order with idempotency. Reads still use [SunflowerApi] directly.
///
/// This wraps the buffer with typed helpers for the app's mutations so callers
/// don't hand-build paths/bodies. The buffer assigns the Idempotency-Key and
/// drains opportunistically; offline calls simply stay queued.
class BufferedApiClient implements RecommendationFeedbackClient {
  BufferedApiClient(this._buffer);

  final ReplayBuffer _buffer;

  /// Live count of unconfirmed mutations.
  Stream<int> watchPendingCount() => _buffer.watchPendingCount();

  /// Buffered drops due to overflow (sync-status surfaced).
  int get overflowDrops => _buffer.overflowDrops;

  /// Manually trigger a drain (the "retry now" action).
  Future<void> retryNow() => _buffer.drain();

  // --- Mutations (queued, then replayed) ------------------------------------

  Future<String> like(
    String mediaId, {
    required bool liked,
    DateTime? occurredAt,
    String? idempotencyKey,
  }) {
    return _buffer.enqueue(
      kind: liked ? 'like' : 'unlike',
      method: 'POST',
      path: '/api/v1/likes',
      idempotencyKey: idempotencyKey,
      body: _likeBody(mediaId, liked: liked, occurredAt: occurredAt),
    );
  }

  Future<String> scrobble({
    required String mediaId,
    required String queueId,
    required int totalPlayedMs,
    required int durationMs,
    required DateTime occurredAt,
    String? idempotencyKey,
  }) {
    return playbackEvent(
      kind: 'play',
      mediaId: mediaId,
      queueId: queueId,
      totalPlayedMs: totalPlayedMs,
      durationMs: durationMs,
      occurredAt: occurredAt,
      idempotencyKey: idempotencyKey,
    );
  }

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
    final key = idempotencyKey ?? IdempotencyKeys().next();
    return _buffer.enqueue(
      kind: 'event',
      method: 'POST',
      path: '/api/v1/events',
      idempotencyKey: key,
      body: {
        'events': [
          _playbackEventBody(
            eventId: key,
            kind: kind,
            mediaId: mediaId,
            queueId: queueId,
            totalPlayedMs: totalPlayedMs,
            durationMs: durationMs,
            occurredAt: occurredAt,
            reason: reason,
          ),
        ],
      },
    );
  }

  Future<String?> logImpressions(
    List<Map<String, dynamic>> impressions, {
    String? idempotencyKey,
  }) {
    if (impressions.isEmpty) return Future.value(null);
    return _buffer.enqueue(
      kind: 'impression',
      method: 'POST',
      path: '/api/v1/impressions',
      idempotencyKey: idempotencyKey,
      body: {'impressions': impressions},
    );
  }

  Future<void> addPlaylistItem(String playlistId, String mediaId) {
    return _buffer.enqueue(
      kind: 'playlist_add',
      method: 'POST',
      path: '/api/v1/playlists/${_pathSegment(playlistId)}/items',
      body: {'media_id': mediaId},
    );
  }

  Future<String> createPlaylist(String title, {String? idempotencyKey}) {
    return _buffer.enqueue(
      kind: 'playlist_create',
      method: 'POST',
      path: '/api/v1/playlists',
      idempotencyKey: idempotencyKey,
      body: {'title': title},
    );
  }

  Future<void> removePlaylistItem(String playlistId, String mediaId) {
    return _buffer.enqueue(
      kind: 'playlist_remove',
      method: 'DELETE',
      path:
          '/api/v1/playlists/${_pathSegment(playlistId)}/items/${_pathSegment(mediaId)}',
    );
  }

  Future<void> renamePlaylist(String playlistId, String title) {
    return _buffer.enqueue(
      kind: 'playlist_rename',
      method: 'PATCH',
      path: '/api/v1/playlists/${_pathSegment(playlistId)}',
      body: {'title': title},
    );
  }

  Future<void> deletePlaylist(String playlistId) {
    return _buffer.enqueue(
      kind: 'playlist_delete',
      method: 'DELETE',
      path: '/api/v1/playlists/${_pathSegment(playlistId)}',
    );
  }

  Future<void> registerDownload({
    required String deviceId,
    required String mediaId,
    required String localPath,
    required int bytes,
  }) {
    return _buffer.enqueue(
      kind: 'download_register',
      method: 'POST',
      path: '/api/v1/devices/${_pathSegment(deviceId)}/downloads',
      body: {'media_id': mediaId, 'local_path': localPath, 'bytes': bytes},
    );
  }

  Future<void> removeDownload(String deviceId, String mediaId) {
    return _buffer.enqueue(
      kind: 'download_remove',
      method: 'DELETE',
      path:
          '/api/v1/devices/${_pathSegment(deviceId)}/downloads/${_pathSegment(mediaId)}',
    );
  }
}

/// Direct recommendation feedback sink for a standalone recommendation server.
///
/// Durability comes from the Rust core recommendation event log: callers only
/// mark local events synced after these requests succeed.
class DirectRecommendationFeedbackClient
    implements RecommendationFeedbackClient {
  DirectRecommendationFeedbackClient({
    required Dio dio,
    IdempotencyKeys? keys,
  })  : _dio = dio,
        _keys = keys ?? IdempotencyKeys();

  final Dio _dio;
  final IdempotencyKeys _keys;

  @override
  Future<String> like(
    String mediaId, {
    required bool liked,
    DateTime? occurredAt,
    String? idempotencyKey,
  }) async {
    final key = idempotencyKey ?? _keys.next();
    await _dio.post<void>(
      '/api/v1/likes',
      data: _likeBody(mediaId, liked: liked, occurredAt: occurredAt),
      options: _idempotencyOptions(key),
    );
    return key;
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
  }) async {
    final key = idempotencyKey ?? _keys.next();
    await _dio.post<void>(
      '/api/v1/events',
      data: {
        'events': [
          _playbackEventBody(
            eventId: key,
            kind: kind,
            mediaId: mediaId,
            queueId: queueId,
            totalPlayedMs: totalPlayedMs,
            durationMs: durationMs,
            occurredAt: occurredAt,
            reason: reason,
          ),
        ],
      },
      options: _idempotencyOptions(key),
    );
    return key;
  }

  @override
  Future<String?> logImpressions(
    List<Map<String, dynamic>> impressions, {
    String? idempotencyKey,
  }) async {
    if (impressions.isEmpty) return null;
    final key = idempotencyKey ?? _keys.next();
    await _dio.post<void>(
      '/api/v1/impressions',
      data: {'impressions': impressions},
      options: _idempotencyOptions(key),
    );
    return key;
  }
}

Map<String, dynamic> _likeBody(
  String mediaId, {
  required bool liked,
  DateTime? occurredAt,
}) =>
    {
      'media_id': mediaId,
      'liked': liked,
      'occurred_at': (occurredAt ?? DateTime.now()).toUtc().toIso8601String(),
    };

Map<String, dynamic> _playbackEventBody({
  required String eventId,
  required String kind,
  required String mediaId,
  required String queueId,
  required int totalPlayedMs,
  required int durationMs,
  required DateTime occurredAt,
  required String reason,
}) {
  final event = <String, dynamic>{
    'event_id': eventId,
    'kind': kind,
    'media_id': mediaId,
    'queue_id': queueId,
    'total_played_ms': totalPlayedMs,
    'duration_ms': durationMs,
    'occurred_at': occurredAt.toUtc().toIso8601String(),
  };
  if (reason.isNotEmpty) event['reason'] = reason;
  return event;
}

Options _idempotencyOptions(String key) {
  return Options(headers: {'Idempotency-Key': key});
}

String _pathSegment(String value) => Uri.encodeComponent(value);
