import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/api/sunflower_api.dart';
import '../../core/db/database_provider.dart';
import '../../core/player/player_bootstrap.dart';
import '../../core/ui/empty_state.dart';
import '../../core/ui/section_rail.dart';
import '../../core/ui/status_banner.dart';
import '../settings/sync_status_widget.dart';
import 'chip_bar.dart';
import 'home_controller.dart';

/// The Home tab: recommendation sections with pull-to-refresh and a cold-start
/// "stale" banner. Tapping a tile starts a server queue seeded by that item and
/// hands it to the audio handler (M4 queue mode).
class HomeScreen extends ConsumerWidget {
  const HomeScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final feedAsync = ref.watch(homeFeedProvider);

    return Scaffold(
      appBar: AppBar(
        title: const Text(
          'Sunflower',
          style: TextStyle(fontWeight: FontWeight.w900),
        ),
        actions: const [
          Padding(
            padding: EdgeInsets.only(right: 16),
            child: Icon(Icons.graphic_eq),
          ),
        ],
      ),
      body: feedAsync.when(
        data: (feed) => RefreshIndicator(
          onRefresh: () async => ref.refresh(homeFeedProvider.future),
          child: _FeedBody(feed: feed),
        ),
        loading: () => const Center(child: CircularProgressIndicator()),
        error: (e, _) => _ErrorView(
          onRetry: () => ref.refresh(homeFeedProvider.future),
        ),
      ),
    );
  }
}

class _FeedBody extends ConsumerWidget {
  const _FeedBody({required this.feed});

  final HomeFeed feed;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return ListView(
      children: [
        const SyncStatusWidget(),
        if (feed.stale) const _StaleBanner(),
        ChipBar(chips: feed.chips),
        for (final section in feed.sections)
          SectionRail(
            section: section,
            onTap: (sec, item, index) => _onTap(ref, sec, item, index),
          ),
        const SizedBox(height: 12),
      ],
    );
  }

  Future<void> _onTap(
    WidgetRef ref,
    HomeSection section,
    HomeItem item,
    int index,
  ) async {
    final api = ref.read(sunflowerApiProvider);
    final db = ref.read(databaseProvider);
    final handler = ref.read(audioHandlerProvider);

    // Log the impression (best effort) so novelty/dedupe can suppress repeats.
    unawaitedLog(
      api.logImpressions([
        {
          'section_id': section.id,
          'source': item.source,
          'seed_id': section.seed ?? '',
          'media_id': item.mediaId,
          'position': index,
        },
      ]),
    );

    if (!item.mediaId.startsWith('yt:')) {
      final song = Song(
        mediaId: item.mediaId,
        sourceType: item.source,
        title: item.title,
        artistName: item.artists.join(', '),
        albumTitle: '',
        hasArt: false,
        albumId: item.albumId,
        durationMs: item.durationMs,
      );
      await handler.loadPlaylist([song], 0, api.streamUrl, api.authHeaders);
      return;
    }

    // Start a server queue seeded by the tapped YouTube item, then play in
    // queue mode.
    final queue = await api.startQueue(seedKind: 'song', seedId: item.mediaId);
    await handler.startQueue(
      api: api,
      db: db,
      queueId: queue.queueId,
      authHeaders: api.authHeaders,
    );
  }
}

/// Fire-and-forget a future, swallowing errors (impression logging is advisory).
void unawaitedLog(Future<void> f) {
  f.catchError((_) {});
}

class _StaleBanner extends StatelessWidget {
  const _StaleBanner();

  @override
  Widget build(BuildContext context) {
    return const StatusBanner(
      icon: Icons.cloud_off,
      text: "Showing saved recommendations — couldn't reach the server.",
    );
  }
}

class _ErrorView extends StatelessWidget {
  const _ErrorView({required this.onRetry});

  final VoidCallback onRetry;

  @override
  Widget build(BuildContext context) {
    return EmptyState(
      icon: Icons.cloud_off,
      title: 'Could not load recommendations',
      message: 'Check your server connection, then try again.',
      action: FilledButton(
        onPressed: onRetry,
        child: const Text('Retry'),
      ),
    );
  }
}
