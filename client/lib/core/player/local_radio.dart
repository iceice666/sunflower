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
  /// paired with its playable local stream URL. Items without a usable
  /// [RecentPlay.streamUrl] are skipped (e.g. expired YT entries we can no
  /// longer resolve offline). One query, no per-item rescans.
  Future<List<({QueueItem item, String url})>> fromRecentPlays({
    int limit = 50,
  }) async {
    final rows = await _db.recentPlays_(limit: limit);
    return [
      for (final r in rows)
        if (r.streamUrl != null && r.streamUrl!.isNotEmpty)
          (
            item: QueueItem(
              mediaId: r.mediaId,
              title: r.title,
              artists: r.artistName.isEmpty ? const [] : [r.artistName],
              durationMs: r.durationMs,
            ),
            url: r.streamUrl!,
          ),
    ];
  }
}
