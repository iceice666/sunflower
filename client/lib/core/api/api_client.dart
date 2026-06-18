import '../sync/replay_buffer.dart';

/// Mutation-routing facade (M7): every mutating call goes through the
/// [ReplayBuffer] first so it is durably queued and replayed in client-clock
/// order with idempotency. Reads still use [SunflowerApi] directly.
///
/// This wraps the buffer with typed helpers for the app's mutations so callers
/// don't hand-build paths/bodies. The buffer assigns the Idempotency-Key and
/// drains opportunistically; offline calls simply stay queued.
class BufferedApiClient {
  BufferedApiClient(this._buffer);

  final ReplayBuffer _buffer;

  /// Live count of unconfirmed mutations.
  Stream<int> watchPendingCount() => _buffer.watchPendingCount();

  /// Buffered drops due to overflow (sync-status surfaced).
  int get overflowDrops => _buffer.overflowDrops;

  /// Manually trigger a drain (the "retry now" action).
  Future<void> retryNow() => _buffer.drain();

  // --- Mutations (queued, then replayed) ------------------------------------

  Future<void> like(String mediaId, {required bool liked}) {
    return _buffer.enqueue(
      kind: liked ? 'like' : 'unlike',
      method: 'POST',
      path: '/api/v1/likes',
      body: {'media_id': mediaId, 'liked': liked},
    );
  }

  Future<void> scrobble({
    required String mediaId,
    required String queueId,
    required int totalPlayedMs,
    required int durationMs,
    required DateTime occurredAt,
  }) {
    return _buffer.enqueue(
      kind: 'event',
      method: 'POST',
      path: '/api/v1/events',
      body: {
        'events': [
          {
            'kind': 'play',
            'media_id': mediaId,
            'queue_id': queueId,
            'total_played_ms': totalPlayedMs,
            'duration_ms': durationMs,
            'occurred_at': occurredAt.toUtc().toIso8601String(),
          },
        ],
      },
    );
  }

  Future<void> addPlaylistItem(String playlistId, String mediaId) {
    return _buffer.enqueue(
      kind: 'playlist_add',
      method: 'POST',
      path: '/api/v1/playlists/$playlistId/items',
      body: {'media_id': mediaId},
    );
  }

  Future<void> removePlaylistItem(String playlistId, String mediaId) {
    return _buffer.enqueue(
      kind: 'playlist_remove',
      method: 'DELETE',
      path: '/api/v1/playlists/$playlistId/items/$mediaId',
    );
  }

  Future<void> renamePlaylist(String playlistId, String title) {
    return _buffer.enqueue(
      kind: 'playlist_rename',
      method: 'PATCH',
      path: '/api/v1/playlists/$playlistId',
      body: {'title': title},
    );
  }

  Future<void> deletePlaylist(String playlistId) {
    return _buffer.enqueue(
      kind: 'playlist_delete',
      method: 'DELETE',
      path: '/api/v1/playlists/$playlistId',
    );
  }
}
