import '../db/database.dart';

class PlaybackSource {
  const PlaybackSource({
    required this.uri,
    required this.expiresAt,
    this.headers,
  });

  factory PlaybackSource.network({
    required String uri,
    required String source,
    required DateTime? expiresAt,
    required Map<String, String> authHeaders,
  }) {
    return PlaybackSource(
      uri: uri,
      headers: source == 'local' ? Map.unmodifiable(authHeaders) : null,
      expiresAt: expiresAt,
    );
  }

  final String uri;
  final Map<String, String>? headers;
  final DateTime? expiresAt;
}

/// Resolves the playable URI for a media id, preferring a local downloaded file
/// over any network URL (M6). The player consults this before using a `/next`
/// or `/streams/resolve` URL so a downloaded track never touches the network —
/// the core of the "airplane-mode playback" acceptance.
class SourceResolver {
  SourceResolver(this._db);

  final SunflowerDatabase _db;

  /// Returns a `file://` URI for [mediaId] when it is downloaded locally, else
  /// null (the caller falls back to the server-provided stream URL).
  Future<String?> localUriFor(String mediaId) async {
    final track = await _db.downloadedTrack(mediaId);
    if (track == null) return null;
    return Uri.file(track.localPath).toString();
  }

  /// Returns a local-file playback source when [mediaId] has been downloaded.
  Future<PlaybackSource?> downloadedSource(String mediaId) async {
    final uri = await localUriFor(mediaId);
    if (uri == null) return null;
    return PlaybackSource(uri: uri, expiresAt: null);
  }

  /// Chooses between the local file and a [networkUrl]: local wins when present.
  /// Returns the URI to hand to just_audio.
  Future<String> resolve(String mediaId, String networkUrl) async {
    return (await localUriFor(mediaId)) ?? networkUrl;
  }

  /// Chooses the full just_audio source configuration for a resolved stream.
  /// Downloaded files never carry auth headers and never expire. Local server
  /// streams keep auth headers; remote/proxy streams use their signed URL.
  Future<PlaybackSource> playbackSourceFor({
    required String mediaId,
    required String networkUrl,
    required String source,
    required DateTime? expiresAt,
    required Map<String, String> authHeaders,
  }) async {
    return await downloadedSource(mediaId) ??
        PlaybackSource.network(
          uri: networkUrl,
          source: source,
          expiresAt: expiresAt,
          authHeaders: authHeaders,
        );
  }
}
