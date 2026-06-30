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
      lookahead:
          look.cast<Map<String, dynamic>>().map(QueueItem.fromJson).toList(),
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
      items:
          items.cast<Map<String, dynamic>>().map(QueueItem.fromJson).toList(),
    );
  }
}

// ---------------------------------------------------------------------------
// M5 home / recommendation models
// ---------------------------------------------------------------------------

/// One rendered recommendation item in a home section.
class HomeItem {
  const HomeItem({
    required this.mediaId,
    required this.title,
    required this.source,
    this.artists = const [],
    this.albumId,
    this.durationMs = 0,
    this.thumbnailUrl,
  });

  final String mediaId;
  final String title;
  final String source;
  final List<String> artists;
  final String? albumId;
  final int durationMs;
  final String? thumbnailUrl;

  factory HomeItem.fromJson(Map<String, dynamic> json) {
    return HomeItem(
      mediaId: json['media_id'] as String? ?? '',
      title: json['title'] as String? ?? '',
      source: json['source'] as String? ?? '',
      artists: (json['artists'] as List<dynamic>? ?? const []).cast<String>(),
      albumId: json['album_id'] as String?,
      durationMs: json['duration_ms'] as int? ?? 0,
      thumbnailUrl: json['thumbnail_url'] as String?,
    );
  }
}

/// A titled row in the home feed (quick_picks, daily_discover, …).
class HomeSection {
  const HomeSection({
    required this.id,
    required this.title,
    required this.kind,
    required this.items,
    this.seed,
  });

  final String id;
  final String title;
  final String kind;
  final String? seed;
  final List<HomeItem> items;

  factory HomeSection.fromJson(Map<String, dynamic> json) {
    final raw = json['items'] as List<dynamic>? ?? const [];
    return HomeSection(
      id: json['id'] as String? ?? '',
      title: json['title'] as String? ?? '',
      kind: json['kind'] as String? ?? '',
      seed: json['seed'] as String?,
      items: raw.cast<Map<String, dynamic>>().map(HomeItem.fromJson).toList(),
    );
  }
}

/// The full `/home` payload: sections, mood/genre chips, and a stale flag set
/// when the feed was served from an expired cache (cold start).
class HomeFeed {
  const HomeFeed({
    required this.sections,
    this.chips = const [],
    this.stale = false,
  });

  final List<HomeSection> sections;
  final List<String> chips;
  final bool stale;

  factory HomeFeed.fromJson(Map<String, dynamic> json) {
    final secs = json['sections'] as List<dynamic>? ?? const [];
    return HomeFeed(
      sections:
          secs.cast<Map<String, dynamic>>().map(HomeSection.fromJson).toList(),
      chips: (json['chips'] as List<dynamic>? ?? const []).cast<String>(),
      stale: json['stale'] as bool? ?? false,
    );
  }
}

// ---------------------------------------------------------------------------
// Search models
// ---------------------------------------------------------------------------

class SearchResults {
  const SearchResults({
    required this.query,
    this.songs = const [],
    this.albums = const [],
    this.artists = const [],
    this.continuation,
  });

  final String query;
  final List<SearchSong> songs;
  final List<SearchAlbum> albums;
  final List<SearchArtist> artists;
  final String? continuation;

  bool get isEmpty => songs.isEmpty && albums.isEmpty && artists.isEmpty;

  factory SearchResults.fromJson(Map<String, dynamic> json) {
    final songs = json['songs'] as List<dynamic>? ?? const [];
    final albums = json['albums'] as List<dynamic>? ?? const [];
    final artists = json['artists'] as List<dynamic>? ?? const [];
    return SearchResults(
      query: json['query'] as String? ?? '',
      songs:
          songs.cast<Map<String, dynamic>>().map(SearchSong.fromJson).toList(),
      albums: albums
          .cast<Map<String, dynamic>>()
          .map(SearchAlbum.fromJson)
          .toList(),
      artists: artists
          .cast<Map<String, dynamic>>()
          .map(SearchArtist.fromJson)
          .toList(),
      continuation: json['continuation'] as String?,
    );
  }
}

class SearchSong {
  const SearchSong({
    required this.mediaId,
    required this.source,
    required this.title,
    this.artists = const [],
    this.thumbnailUrl,
    this.durationMs = 0,
  });

  final String mediaId;
  final String source;
  final String title;
  final List<String> artists;
  final String? thumbnailUrl;
  final int durationMs;

  factory SearchSong.fromJson(Map<String, dynamic> json) {
    return SearchSong(
      mediaId: json['media_id'] as String? ?? '',
      source: json['source'] as String? ?? '',
      title: json['title'] as String? ?? '',
      artists: (json['artists'] as List<dynamic>? ?? const []).cast<String>(),
      thumbnailUrl: json['thumbnail_url'] as String?,
      durationMs: json['duration_ms'] as int? ?? 0,
    );
  }
}

class SearchAlbum {
  const SearchAlbum({
    required this.browseId,
    required this.title,
    this.artists = const [],
    this.thumbnailUrl,
  });

  final String browseId;
  final String title;
  final List<String> artists;
  final String? thumbnailUrl;

  factory SearchAlbum.fromJson(Map<String, dynamic> json) {
    return SearchAlbum(
      browseId: json['browse_id'] as String? ?? '',
      title: json['title'] as String? ?? '',
      artists: (json['artists'] as List<dynamic>? ?? const []).cast<String>(),
      thumbnailUrl: json['thumbnail_url'] as String?,
    );
  }
}

class SearchArtist {
  const SearchArtist({
    required this.browseId,
    required this.name,
    this.thumbnailUrl,
  });

  final String browseId;
  final String name;
  final String? thumbnailUrl;

  factory SearchArtist.fromJson(Map<String, dynamic> json) {
    return SearchArtist(
      browseId: json['browse_id'] as String? ?? '',
      name: json['name'] as String? ?? '',
      thumbnailUrl: json['thumbnail_url'] as String?,
    );
  }
}

/// A user playlist summary (and items when fetched by id).
class Playlist {
  const Playlist({
    required this.id,
    required this.title,
    required this.version,
    this.items = const [],
  });

  final String id;
  final String title;
  final int version;
  final List<HomeItem> items;

  factory Playlist.fromJson(Map<String, dynamic> json) {
    final raw = json['items'] as List<dynamic>? ?? const [];
    return Playlist(
      id: json['id'] as String? ?? '',
      title: json['title'] as String? ?? '',
      version: (json['version'] as num?)?.toInt() ?? 0,
      items: raw.cast<Map<String, dynamic>>().map(HomeItem.fromJson).toList(),
    );
  }
}

/// Server-computed SHA-256 of a local song file, for download verification.
class SongHash {
  const SongHash({required this.mediaId, required this.sha256, this.bytes = 0});

  final String mediaId;
  final String sha256;
  final int bytes;

  factory SongHash.fromJson(Map<String, dynamic> json) {
    return SongHash(
      mediaId: json['media_id'] as String? ?? '',
      sha256: json['sha256'] as String? ?? '',
      bytes: (json['bytes'] as num?)?.toInt() ?? 0,
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
    return raw.cast<Map<String, dynamic>>().map(Song.fromJson).toList();
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

  // -------------------------------------------------------------------------
  // M5 home / likes / playlists / impressions
  // -------------------------------------------------------------------------

  /// Fetches the recommendation home feed, honoring filter [prefs].
  Future<HomeFeed> home({
    bool hideExplicit = false,
    bool hideVideo = false,
    bool hideShorts = false,
  }) async {
    final response = await _dio.get<Map<String, dynamic>>(
      '/api/v1/home',
      queryParameters: {
        'hide_explicit': hideExplicit,
        'hide_video': hideVideo,
        'hide_shorts': hideShorts,
      },
    );
    return HomeFeed.fromJson(response.data ?? const {});
  }

  /// Searches YouTube Music through the authenticated server.
  Future<SearchResults> search(String query, {int limit = 20}) async {
    final response = await _dio.get<Map<String, dynamic>>(
      '/api/v1/search',
      queryParameters: {'q': query, 'limit': limit},
    );
    return SearchResults.fromJson(response.data ?? const {});
  }

  /// Toggles a like for [mediaId]. Mutating — carries an Idempotency-Key.
  Future<bool> toggleLike(String mediaId, bool liked) async {
    final response = await _dio.post<Map<String, dynamic>>(
      '/api/v1/likes',
      data: {'media_id': mediaId, 'liked': liked},
      options: Options(headers: {'Idempotency-Key': const Uuid().v4()}),
    );
    return response.data?['liked'] as bool? ?? liked;
  }

  /// Logs a batch of recommendation impressions (best effort).
  Future<void> logImpressions(List<Map<String, dynamic>> impressions) async {
    if (impressions.isEmpty) return;
    await _dio.post<Map<String, dynamic>>(
      '/api/v1/impressions',
      data: {'impressions': impressions},
    );
  }

  /// Lists the user's playlists (summaries, no items).
  Future<List<Playlist>> listPlaylists() async {
    final response = await _dio.get<Map<String, dynamic>>('/api/v1/playlists');
    final raw = response.data?['playlists'] as List<dynamic>? ?? const [];
    return raw.cast<Map<String, dynamic>>().map(Playlist.fromJson).toList();
  }

  /// Fetches a single playlist with its items.
  Future<Playlist> getPlaylist(String id) async {
    final response = await _dio.get<Map<String, dynamic>>(
      '/api/v1/playlists/$id',
    );
    return Playlist.fromJson(response.data ?? const {});
  }

  /// Creates a playlist and returns its summary.
  Future<Playlist> createPlaylist(String title) async {
    final response = await _dio.post<Map<String, dynamic>>(
      '/api/v1/playlists',
      data: {'title': title},
      options: Options(headers: {'Idempotency-Key': const Uuid().v4()}),
    );
    return Playlist.fromJson(response.data ?? const {});
  }

  /// Adds a song to a playlist.
  Future<void> addPlaylistItem(String playlistId, String mediaId) async {
    await _dio.post<void>(
      '/api/v1/playlists/$playlistId/items',
      data: {'media_id': mediaId},
      options: Options(headers: {'Idempotency-Key': const Uuid().v4()}),
    );
  }

  // -------------------------------------------------------------------------
  // M6 offline downloads
  // -------------------------------------------------------------------------

  /// Fetches the server-computed SHA-256 of a local-library song for download
  /// verification. Throws (DioException) for non-local songs (404).
  Future<SongHash> songHash(String mediaId) async {
    final response = await _dio.get<Map<String, dynamic>>(
      '/api/v1/library/songs/$mediaId/hash',
    );
    return SongHash.fromJson(response.data ?? const {});
  }

  /// Registers a completed download with the server's per-device registry.
  /// Mutating — carries an Idempotency-Key.
  Future<void> registerDownload({
    required String deviceId,
    required String mediaId,
    required String localPath,
    required int bytes,
  }) async {
    await _dio.post<void>(
      '/api/v1/devices/$deviceId/downloads',
      data: {'media_id': mediaId, 'local_path': localPath, 'bytes': bytes},
      options: Options(headers: {'Idempotency-Key': const Uuid().v4()}),
    );
  }

  /// Removes a download registration from the server.
  Future<void> deleteDownload(String deviceId, String mediaId) async {
    await _dio.delete<void>('/api/v1/devices/$deviceId/downloads/$mediaId');
  }

  /// Authorization header map — pass to just_audio and cached_network_image.
  Map<String, String> get authHeaders => {'Authorization': 'Bearer $_token'};
}
