import 'dart:async';

import '../api/sunflower_api.dart';
import '../db/database.dart';
import 'package:drift/drift.dart' show Value;

/// Minimum number of buffered (resolved) items the player keeps ahead of the
/// current track. When the buffer drops below this, the loader fetches the next
/// `/next` page in the background. Matches the M4 acceptance criterion of a
/// buffer maintained ≥4 items under simulated playback.
const kMinBuffer = 4;

/// LookaheadLoader owns the server-queue cursor and fills a buffer of upcoming
/// [QueueItem]s by paging `GET /api/v1/next`. Newer servers may include
/// playable stream fields on lookahead entries; older metadata-only entries are
/// resolved later by the audio handler. The loader mirrors both forms into
/// [LookaheadCache] for cold-start.
///
/// The loader is deliberately transport-only: no just_audio dependency, so it
/// is unit-testable against a mock [SunflowerApi] and an in-memory database.
class LookaheadLoader {
  LookaheadLoader({
    required SunflowerApi api,
    required SunflowerDatabase db,
    required String queueId,
  })  : _api = api,
        _db = db,
        _queueId = queueId;

  final SunflowerApi _api;
  final SunflowerDatabase _db;
  final String _queueId;

  /// The buffered upcoming items (excludes the current track). Ordered.
  final List<QueueItem> _buffer = [];

  /// Server position of the *last* item we have buffered. The next page request
  /// starts after this. Begins at the current track position.
  int _cursor = 0;
  bool _hasMore = true;
  bool _loading = false;

  String get queueId => _queueId;
  bool get hasMore => _hasMore;
  int get bufferLength => _buffer.length;
  List<QueueItem> get buffered => List.unmodifiable(_buffer);

  /// Loads the initial window around [position], returning the resolved current
  /// track (or null if the queue is exhausted/unreachable). Seeds the buffer
  /// with the first lookahead window.
  Future<ResolvedStream?> start(int position) async {
    _cursor = position;
    _buffer.clear();
    final resp = await _api.next(queueId: _queueId, position: position);
    _ingest(resp);
    return resp.current;
  }

  /// Ensures at least [kMinBuffer] items are buffered, fetching one page if
  /// needed. Safe to call repeatedly; concurrent calls coalesce. Returns the
  /// items newly appended to the buffer (may be empty).
  Future<List<QueueItem>> ensureBuffer() async {
    if (_loading || !_hasMore || _buffer.length >= kMinBuffer) {
      return const [];
    }
    _loading = true;
    try {
      final before = _buffer.length;
      // The server returns lookahead starting at requestPosition+1 (it drops
      // `current`). _cursor is the absolute position of the last buffered item,
      // so request _cursor to receive items[_cursor+1 ...] with no gap.
      final resp = await _api.next(queueId: _queueId, position: _cursor);
      _ingest(resp);
      return _buffer.sublist(before.clamp(0, _buffer.length));
    } finally {
      _loading = false;
    }
  }

  /// Pops the next buffered item, advancing playback. Returns null when empty.
  QueueItem? takeNext() {
    if (_buffer.isEmpty) return null;
    return _buffer.removeAt(0);
  }

  void _ingest(NextResponse resp) {
    _hasMore = resp.hasMore;
    if (resp.lookahead.isEmpty) {
      // No items past current: advance cursor only if current is present so a
      // re-poll doesn't refetch the same empty window forever.
      if (resp.current != null) _cursor = resp.position;
      return;
    }
    _buffer.addAll(resp.lookahead);
    _cursor = resp.position + resp.lookahead.length;
    unawaited(_persist(resp));
  }

  /// Mirrors the current window into the Drift cache for cold-start. Best
  /// effort: failures here never affect playback.
  Future<void> _persist(NextResponse resp) async {
    final rows = <LookaheadCacheCompanion>[];
    var pos = resp.position;
    final cur = resp.current;
    if (cur != null) {
      rows.add(
        _companion(
          pos,
          cur.mediaId,
          cur.title,
          cur.artists,
          cur.durationMs,
          cur.source,
          streamUrl: cur.streamUrl,
          streamExpiresAt: cur.expiresAt,
          mimeType: cur.mimeType,
        ),
      );
      pos++;
    }
    for (final it in resp.lookahead) {
      final stream = it.resolvedStream;
      rows.add(
        _companion(
          pos,
          it.mediaId,
          it.title,
          it.artists,
          it.durationMs,
          stream?.source ?? '',
          streamUrl: stream?.streamUrl,
          streamExpiresAt: stream?.expiresAt,
          mimeType: stream?.mimeType,
        ),
      );
      pos++;
    }
    try {
      await _db.replaceLookahead(_queueId, rows);
    } catch (_) {
      // Cache is advisory; ignore write failures.
    }
  }

  LookaheadCacheCompanion _companion(
    int position,
    String mediaId,
    String title,
    List<String> artists,
    int durationMs,
    String source, {
    String? streamUrl,
    DateTime? streamExpiresAt,
    String? mimeType,
  }) {
    return LookaheadCacheCompanion.insert(
      queueId: _queueId,
      position: position,
      mediaId: mediaId,
      title: Value(title),
      artistsJson: Value(_encodeArtists(artists)),
      durationMs: Value(durationMs),
      source: Value(source),
      streamUrl: Value(streamUrl),
      streamExpiresAt: Value(streamExpiresAt),
      mimeType: Value(mimeType),
    );
  }
}

String _encodeArtists(List<String> artists) {
  // Minimal JSON array encoding; values are plain strings from the server.
  final escaped = artists.map((a) => '"${a.replaceAll('"', '\\"')}"');
  return '[${escaped.join(',')}]';
}
