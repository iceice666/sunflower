import '../db/database.dart';

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

  /// Chooses between the local file and a [networkUrl]: local wins when present.
  /// Returns the URI to hand to just_audio.
  Future<String> resolve(String mediaId, String networkUrl) async {
    return (await localUriFor(mediaId)) ?? networkUrl;
  }
}
