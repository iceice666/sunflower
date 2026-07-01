import '../api/sunflower_api.dart';
import '../db/database.dart';

/// LocalRadio builds an offline fallback queue from the device's own play
/// history when the server is unreachable and the lookahead buffer is
/// exhausted. It is the last line of defence in the M4 "disconnect mid-playback"
/// scenario: 5 buffered tracks play out, then local radio kicks in.
///
/// The source is [RecentPlays] (recent + frequently played). Only items with a
/// usable local/cached stream URL are returned, since by definition there is no
/// network to resolve YouTube URLs at this point.
class LocalRadio {
  LocalRadio(this._db);

  final SunflowerDatabase _db;

  /// Returns up to [limit] fallback entries ordered most-recent first, each
  /// paired with a playable local URL. A reusable local stream URL wins; when
  /// it is absent, a completed download is still playable via file://.
  Future<List<({QueueItem item, String url})>> fromRecentPlays({
    int limit = 50,
  }) async {
    final rows = await _db.recentPlays_(limit: limit);
    final out = <({QueueItem item, String url})>[];
    for (final r in rows) {
      final url = await _playableUrlFor(r);
      if (url == null) continue;
      out.add(
        (
          item: QueueItem(
            mediaId: r.mediaId,
            title: r.title,
            artists: r.artistName.isEmpty ? const [] : [r.artistName],
            durationMs: r.durationMs,
          ),
          url: url,
        ),
      );
    }
    return out;
  }

  Future<String?> _playableUrlFor(RecentPlay play) async {
    if (_hasReusableLocalStream(play)) {
      return play.streamUrl;
    }
    final downloaded = await _db.downloadedTrack(play.mediaId);
    if (downloaded == null) return null;
    return Uri.file(downloaded.localPath).toString();
  }
}

bool _hasReusableLocalStream(RecentPlay play) {
  if (play.streamUrl == null || play.streamUrl!.isEmpty) return false;
  return play.source == 'local' ||
      play.mediaId.startsWith('local:') ||
      Uri.tryParse(play.streamUrl!)?.scheme == 'file';
}
