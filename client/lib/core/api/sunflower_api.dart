import 'package:dio/dio.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:uuid/uuid.dart';

import '../auth/token_store.dart';

// ---------------------------------------------------------------------------
// Song model
// ---------------------------------------------------------------------------

/// A song as returned by GET /api/v1/library/songs.
///
/// The server emits a clean JSON shape (no pgtype wrappers):
///   - [mediaId], [title], [artistName], [albumTitle] are always plain strings.
///   - [albumId] and [durationMs] are nullable (null when the song has no album
///     or the scanner couldn't determine the duration).
///   - [hasArt] is true when the server has a cached cover for this song's album.
class Song {
  const Song({
    required this.mediaId,
    required this.sourceType,
    required this.title,
    required this.artistName,
    required this.albumTitle,
    required this.hasArt,
    this.albumId,
    this.durationMs,
  });

  final String mediaId;
  final String sourceType;
  final String title;
  final String artistName;
  final String albumTitle;
  final bool hasArt;
  final String? albumId;
  final int? durationMs;

  factory Song.fromJson(Map<String, dynamic> json) {
    return Song(
      mediaId: json['media_id'] as String,
      sourceType: json['source_type'] as String,
      title: json['title'] as String,
      artistName: json['artist_name'] as String? ?? '',
      albumTitle: json['album_title'] as String? ?? '',
      hasArt: json['has_art'] as bool? ?? false,
      albumId: json['album_id'] as String?,
      durationMs: json['duration_ms'] as int?,
    );
  }

  @override
  String toString() => 'Song($mediaId, $title)';
}

// ---------------------------------------------------------------------------
// M4 queue / stream models
// ---------------------------------------------------------------------------

/// A resolved, playable stream — the shape of `/next.current` and the
/// `/streams/resolve` response. [expiresAt] is null for local sources and set
/// (RFC3339) for YouTube/proxy sources that the expiry guard must refresh.
class ResolvedStream {
  const ResolvedStream({
    required this.mediaId,
    required this.source,
    required this.streamUrl,
    this.title = '',
    this.artists = const [],
    this.durationMs = 0,
    this.expiresAt,
    this.mimeType = '',
  });

  final String mediaId;
  final String source; // local | youtube | proxy
  final String streamUrl;
  final String title;
  final List<String> artists;
  final int durationMs;
  final DateTime? expiresAt;
  final String mimeType;

  factory ResolvedStream.fromJson(Map<String, dynamic> json) {
    final rawExpiry = json['stream_expires_at'] as String?;
    return ResolvedStream(
      mediaId: json['media_id'] as String? ?? '',
      source: json['source'] as String? ?? '',
      streamUrl: json['stream_url'] as String? ?? '',
      title: json['title'] as String? ?? '',
      artists: (json['artists'] as List<dynamic>? ?? const []).cast<String>(),
      durationMs: json['duration_ms'] as int? ?? 0,
      expiresAt: rawExpiry == null ? null : DateTime.tryParse(rawExpiry),
      mimeType: json['mime_type'] as String? ?? '',
    );
  }
}

/// An upcoming, not-yet-resolved queue entry — the shape of `/next.lookahead[]`
/// and `/queue.items[]`. The client resolves each to a [ResolvedStream] as it
/// fills the playback buffer.
class QueueItem {
  const QueueItem({
    required this.mediaId,
    this.title = '',
    this.artists = const [],
    this.durationMs = 0,
  });

  final String mediaId;
  final String title;
  final List<String> artists;
  final int durationMs;

  factory QueueItem.fromJson(Map<String, dynamic> json) {
    return QueueItem(
      mediaId: json['media_id'] as String? ?? '',
      title: json['title'] as String? ?? '',
      artists: (json['artists'] as List<dynamic>? ?? const []).cast<String>(),
      durationMs: json['duration_ms'] as int? ?? 0,
    );
  }
}

/// The GET /api/v1/next payload: a resolved [current] plus a window of
/// unresolved [lookahead] items and a [hasMore] flag for continuation.
class NextResponse {
  const NextResponse({
    required this.queueId,
    required this.position,
    required this.current,
    required this.lookahead,
    required this.hasMore,
  });

  final String queueId;
  final int position;
  final ResolvedStream? current;
  final List<QueueItem> lookahead;
  final bool hasMore;

  factory NextResponse.fromJson(Map<String, dynamic> json) {
    final cur = json['current'] as Map<String, dynamic>?;
    final look = json['lookahead'] as List<dynamic>? ?? const [];
    return NextResponse(
      queueId: json['queue_id'] as String? ?? '',
      position: json['position'] as int? ?? 0,
      current: cur == null ? null : ResolvedStream.fromJson(cur),
      lookahead: look
          .cast<Map<String, dynamic>>()
          .map(QueueItem.fromJson)
          .toList(),
      hasMore: json['has_more'] as bool? ?? false,
    );
  }
}

/// The POST /api/v1/queue/start response: a freshly materialized queue.
class QueueResponse {
  const QueueResponse({
    required this.queueId,
    required this.seedKind,
    required this.version,
    required this.items,
    this.title = '',
  });

  final String queueId;
  final String seedKind;
  final String title;
  final int version;
  final List<QueueItem> items;

  factory QueueResponse.fromJson(Map<String, dynamic> json) {
    final items = json['items'] as List<dynamic>? ?? const [];
    return QueueResponse(
      queueId: json['queue_id'] as String? ?? '',
      seedKind: json['seed_kind'] as String? ?? '',
      title: json['title'] as String? ?? '',
      version: (json['version'] as num?)?.toInt() ?? 0,
      items: items
          .cast<Map<String, dynamic>>()
          .map(QueueItem.fromJson)
          .toList(),
    );
  }
}

// ---------------------------------------------------------------------------
// API client
// ---------------------------------------------------------------------------

/// Riverpod provider for [SunflowerApi]. Initialised lazily from stored
/// credentials. Callers that need to use the API before credentials are loaded
/// should await [serverUrlProvider] and [tokenProvider] first.
final sunflowerApiProvider = Provider<SunflowerApi>((ref) {
  // Read cached values synchronously (may be null on first load — callers
  // guard before use or watch the FutureProviders upstream).
  final serverUrl = ref.watch(serverUrlProvider).valueOrNull ?? '';
  final token = ref.watch(tokenProvider).valueOrNull ?? '';
  return SunflowerApi(baseUrl: serverUrl, token: token);
});

/// Thin Dio wrapper exposing the M2 API surface:
///  - GET  /api/v1/library/songs
///  - Stream URL builder
///  - Art URL builder
class SunflowerApi {
  SunflowerApi({required String baseUrl, required String token})
      : _baseUrl = baseUrl,
        _token = token,
        _dio = Dio(
          BaseOptions(
            baseUrl: baseUrl,
            connectTimeout: const Duration(seconds: 10),
            receiveTimeout: const Duration(seconds: 30),
          ),
        ) {
    if (token.isNotEmpty) {
      _dio.interceptors.add(
        InterceptorsWrapper(
          onRequest: (options, handler) {
            options.headers['Authorization'] = 'Bearer $token';
            handler.next(options);
          },
        ),
      );
    }
  }

  final String _baseUrl;
  final String _token;
  final Dio _dio;

  /// Fetches the full song list (up to 100 per page; M2 doesn't paginate the UI).
  Future<List<Song>> listSongs({int limit = 100, int offset = 0}) async {
    final response = await _dio.get<Map<String, dynamic>>(
      '/api/v1/library/songs',
      queryParameters: {'limit': limit, 'offset': offset},
    );
    final raw = response.data?['songs'] as List<dynamic>? ?? [];
    return raw
        .cast<Map<String, dynamic>>()
        .map(Song.fromJson)
        .toList();
  }

  /// Returns the authenticated stream URL for [mediaId].
  /// The client passes this to just_audio with an Authorization header.
  String streamUrl(String mediaId) =>
      '$_baseUrl/api/v1/library/songs/$mediaId/stream';

  /// Returns the art URL for [albumId] at [size] (256 | 512 | 1024).
  String artUrl(String albumId, {int size = 512}) =>
      '$_baseUrl/api/v1/library/albums/$albumId/art?size=$size';

  // -------------------------------------------------------------------------
  // M4 queue / next / streams
  // -------------------------------------------------------------------------

  /// Starts a server queue from [seedKind] ("song" | "shuffle_liked") and an
  /// optional [seedId]. Returns the materialized queue (≥10 items for a song
  /// seed). Mutating call — carries an Idempotency-Key (full replay buffering
  /// arrives in M7).
  Future<QueueResponse> startQueue({
    required String seedKind,
    String seedId = '',
    String title = '',
  }) async {
    final response = await _dio.post<Map<String, dynamic>>(
      '/api/v1/queue/start',
      data: {'seed_kind': seedKind, 'seed_id': seedId, 'title': title},
      options: Options(headers: {'Idempotency-Key': const Uuid().v4()}),
    );
    return QueueResponse.fromJson(response.data ?? const {});
  }

  /// Fetches the current track plus lookahead for [queueId] at [position].
  Future<NextResponse> next({required String queueId, int position = 0}) async {
    final response = await _dio.get<Map<String, dynamic>>(
      '/api/v1/next',
      queryParameters: {'queue_id': queueId, 'position': position},
    );
    return NextResponse.fromJson(response.data ?? const {});
  }

  /// Re-resolves [mediaId] to a fresh playable stream after a 403 / expiry.
  /// Set [proxy] to force the server proxy (the 403/CORS fallback path).
  /// Mutating call — carries an Idempotency-Key.
  Future<ResolvedStream> resolveStream(
    String mediaId, {
    bool proxy = false,
  }) async {
    final response = await _dio.post<Map<String, dynamic>>(
      '/api/v1/streams/resolve',
      data: {'media_id': mediaId, 'proxy': proxy},
      options: Options(headers: {'Idempotency-Key': const Uuid().v4()}),
    );
    return ResolvedStream.fromJson(response.data ?? const {});
  }

  /// Authorization header map — pass to just_audio and cached_network_image.
  Map<String, String> get authHeaders => {'Authorization': 'Bearer $_token'};
}
