import 'dart:convert';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/api/sunflower_api.dart';
import '../../core/db/database.dart';
import '../../core/db/database_provider.dart';

/// Filter preferences for the home feed, wired from the settings screen.
class HomePrefs {
  const HomePrefs({
    this.hideExplicit = false,
    this.hideVideo = false,
    this.hideShorts = false,
  });

  final bool hideExplicit;
  final bool hideVideo;
  final bool hideShorts;

  /// Cache key suffix so a prefs change doesn't render the wrong cached feed.
  String get cacheKey =>
      'home:${hideExplicit ? 1 : 0}${hideVideo ? 1 : 0}${hideShorts ? 1 : 0}';
}

final homePrefsProvider = StateProvider<HomePrefs>((ref) => const HomePrefs());

/// Fetches the home feed with a cold-start cache fallback:
///   - On success, the feed is cached (HomeCache) and returned.
///   - On failure (server unreachable), the last cached feed is returned with
///     stale=true so the UI shows yesterday's sections plus a "stale" banner.
final homeFeedProvider = FutureProvider.autoDispose<HomeFeed>((ref) async {
  final api = ref.watch(sunflowerApiProvider);
  final db = ref.watch(databaseProvider);
  final prefs = ref.watch(homePrefsProvider);

  try {
    final feed = await api.home(
      hideExplicit: prefs.hideExplicit,
      hideVideo: prefs.hideVideo,
      hideShorts: prefs.hideShorts,
    );
    // Cache the rendered feed for cold start (best effort).
    await db.putHome(prefs.cacheKey, jsonEncode(_feedToJson(feed)));
    return feed;
  } catch (_) {
    final cached = await db.getHome(prefs.cacheKey);
    if (cached != null) {
      final json = jsonDecode(cached.payloadJson) as Map<String, dynamic>;
      final feed = HomeFeed.fromJson(json);
      // Force stale so the UI surfaces the cold-start indicator.
      return HomeFeed(sections: feed.sections, chips: feed.chips, stale: true);
    }
    rethrow;
  }
});

/// Serializes a HomeFeed back to the wire JSON shape for caching.
Map<String, dynamic> _feedToJson(HomeFeed feed) => {
  'sections': [
    for (final s in feed.sections)
      {
        'id': s.id,
        'title': s.title,
        'kind': s.kind,
        if (s.seed != null) 'seed': s.seed,
        'items': [
          for (final it in s.items)
            {
              'media_id': it.mediaId,
              'title': it.title,
              'source': it.source,
              'artists': it.artists,
              if (it.albumId != null) 'album_id': it.albumId,
              'duration_ms': it.durationMs,
              if (it.thumbnailUrl != null) 'thumbnail_url': it.thumbnailUrl,
            },
        ],
      },
  ],
  'chips': feed.chips,
  'stale': feed.stale,
};
