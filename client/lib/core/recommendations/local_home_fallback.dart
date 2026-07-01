import '../api/sunflower_api.dart';
import '../bridge/api.dart' as bridge;
import '../bridge/frb_generated.dart';
import '../db/database.dart';

typedef LocalRanker = Future<List<bridge.RecommendationCandidateDto>?>
    Function({
  required List<bridge.RecommendationCandidateDto> candidates,
  required bridge.LocalStatsSnapshotDto stats,
  required int limit,
});

typedef LocalStatsLoader = Future<bridge.LocalStatsSnapshotDto?> Function({
  required int recentLimit,
});

/// Builds a Home feed from device-local state when the recommendation server
/// and the cached server feed are both unavailable.
class LocalHomeFallback {
  const LocalHomeFallback({
    this.useRustBridge = true,
    this.ranker,
    this.statsLoader,
  });

  final bool useRustBridge;
  final LocalRanker? ranker;
  final LocalStatsLoader? statsLoader;

  static var _rustInitAttempted = false;
  static var _rustAvailable = false;

  Future<HomeFeed?> build(SunflowerDatabase db, {int limit = 20}) async {
    final rows = await db.recentPlays_(limit: 100);
    final downloadedIds = await db.downloadedMediaIds(
      rows.map((row) => row.mediaId),
    );
    final playable = <_PlayableRecent>[];
    for (final row in rows) {
      final downloaded = downloadedIds.contains(row.mediaId);
      if (_hasReusableLocalStream(row) || downloaded) {
        playable.add(_PlayableRecent(row: row, downloaded: downloaded));
      }
    }
    if (playable.isEmpty) return null;

    final ranked = useRustBridge
        ? await _rankWithRust(playable, limit).catchError((_) => null)
        : null;
    final items = ranked ?? _rankWithDart(playable, limit);
    if (items.isEmpty) return null;

    return HomeFeed(
      stale: true,
      chips: const ['local'],
      sections: [
        HomeSection(
          id: 'local_quick_picks',
          title: 'Local Picks',
          kind: 'local_quick_picks',
          items: items,
        ),
      ],
    );
  }

  Future<List<HomeItem>?> _rankWithRust(
    List<_PlayableRecent> rows,
    int limit,
  ) async {
    final candidates = [
      for (final entry in rows)
        bridge.RecommendationCandidateDto(
          mediaId: entry.row.mediaId,
          title: entry.row.title,
          artists:
              entry.row.artistName.isEmpty ? const [] : [entry.row.artistName],
          durationMs: entry.row.durationMs,
          source: bridge.RecommendationSourceDto.local,
          remoteScore: 0,
          reason: 'local',
        ),
    ];
    final stats = await _statsSnapshot(rows);
    final testRanker = ranker;
    if (testRanker != null) {
      final ranked = await testRanker(
        candidates: candidates,
        stats: stats,
        limit: limit,
      );
      return ranked?.map(_homeItemFromCandidate).toList();
    }

    if (!_rustInitAttempted) {
      _rustInitAttempted = true;
      try {
        await RustLib.init();
        _rustAvailable = true;
      } catch (_) {
        _rustAvailable = false;
      }
    }
    if (!_rustAvailable) return null;

    final ranked = await bridge.rankLocalCandidates(
      candidates: candidates,
      stats: stats,
      limit: limit,
    );

    return ranked.map(_homeItemFromCandidate).toList();
  }

  Future<bridge.LocalStatsSnapshotDto> _statsSnapshot(
    List<_PlayableRecent> rows,
  ) async {
    final loadStats = statsLoader;
    if (loadStats != null) {
      try {
        final stats = await loadStats(recentLimit: 10);
        if (stats != null && stats.tracks.isNotEmpty) {
          return stats;
        }
      } catch (_) {
        // Rust-core stats are advisory; Drift recent plays remain the fallback.
      }
    }
    return bridge.LocalStatsSnapshotDto(
      generatedAtMs: DateTime.now().millisecondsSinceEpoch,
      tracks: [
        for (final entry in rows)
          bridge.TrackStatsDto(
            mediaId: entry.row.mediaId,
            playCount: entry.row.playCount,
            skipCount: 0,
            completionCount: entry.row.playCount,
            impressionCount: 0,
            liked: false,
            downloaded: entry.downloaded,
            localAvailable: entry.localAvailable,
            lastPlayedAtMs: entry.row.lastPlayedAt.millisecondsSinceEpoch,
          ),
      ],
      recentMediaIds: [
        for (final entry in rows.take(3)) entry.row.mediaId,
      ],
      recentArtistNames: [
        for (final entry in rows.take(5))
          if (entry.row.artistName.isNotEmpty) entry.row.artistName,
      ],
    );
  }

  List<HomeItem> _rankWithDart(List<_PlayableRecent> rows, int limit) {
    final sorted = [...rows]..sort((a, b) {
        final byPlays = b.row.playCount.compareTo(a.row.playCount);
        if (byPlays != 0) return byPlays;
        final byRecent = b.row.lastPlayedAt.compareTo(a.row.lastPlayedAt);
        if (byRecent != 0) return byRecent;
        return a.row.mediaId.compareTo(b.row.mediaId);
      });
    return sorted
        .take(limit)
        .map((entry) => _homeItemFromRecentPlay(entry.row))
        .toList();
  }
}

class _PlayableRecent {
  const _PlayableRecent({required this.row, required this.downloaded});

  final RecentPlay row;
  final bool downloaded;

  bool get localAvailable =>
      downloaded || _isLocalSource(row) || _isFileUrl(row.streamUrl);
}

HomeItem _homeItemFromCandidate(bridge.RecommendationCandidateDto candidate) {
  return HomeItem(
    mediaId: candidate.mediaId,
    title: candidate.title,
    source: _sourceFromMediaId(candidate.mediaId),
    artists: candidate.artists,
    albumId: candidate.albumId,
    durationMs: candidate.durationMs,
  );
}

HomeItem _homeItemFromRecentPlay(RecentPlay row) {
  return HomeItem(
    mediaId: row.mediaId,
    title: row.title,
    source: row.source.isEmpty ? _sourceFromMediaId(row.mediaId) : row.source,
    artists: row.artistName.isEmpty ? const [] : [row.artistName],
    durationMs: row.durationMs,
  );
}

String _sourceFromMediaId(String mediaId) {
  final sep = mediaId.indexOf(':');
  return sep <= 0 ? 'local' : mediaId.substring(0, sep);
}

bool _isFileUrl(String? url) =>
    url != null && Uri.tryParse(url)?.scheme == 'file';

bool _hasReusableLocalStream(RecentPlay row) {
  return row.streamUrl != null &&
      row.streamUrl!.isNotEmpty &&
      (_isLocalSource(row) || _isFileUrl(row.streamUrl));
}

bool _isLocalSource(RecentPlay row) {
  return row.source == 'local' || row.mediaId.startsWith('local:');
}
