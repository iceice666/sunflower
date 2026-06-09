import 'package:dio/dio.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

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

  /// Authorization header map — pass to just_audio and cached_network_image.
  Map<String, String> get authHeaders => {'Authorization': 'Bearer $_token'};
}
