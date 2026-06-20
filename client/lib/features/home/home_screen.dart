import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/api/sunflower_api.dart';
import '../../core/db/database_provider.dart';
import '../../core/player/player_bootstrap.dart';
import '../settings/sync_status_widget.dart';
import '../player_ui/mini_player.dart';
import 'chip_bar.dart';
import 'home_controller.dart';
import 'section_widget.dart';

/// The Home tab: recommendation sections with pull-to-refresh and a cold-start
/// "stale" banner. Tapping a tile starts a server queue seeded by that item and
/// hands it to the audio handler (M4 queue mode).
class HomeScreen extends ConsumerWidget {
  const HomeScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final feedAsync = ref.watch(homeFeedProvider);

    return Scaffold(
      appBar: AppBar(title: const Text('Home')),
      body: Column(
        children: [
          Expanded(
            child: feedAsync.when(
              data: (feed) => RefreshIndicator(
                onRefresh: () async => ref.refresh(homeFeedProvider.future),
                child: _FeedBody(feed: feed),
              ),
              loading: () => const Center(child: CircularProgressIndicator()),
              error: (e, _) => _ErrorView(
                onRetry: () => ref.refresh(homeFeedProvider.future),
              ),
            ),
          ),
          const MiniPlayer(),
        ],
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
          SectionWidget(
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

    // Start a server queue seeded by the tapped item, then play in queue mode.
    // The server's queue/start "song" seed expands a YouTube video into radio;
    // local-library items have no radio seed kind yet (album/artist/local seeds
    // are deferred — see M4 status), so we only auto-queue YouTube items here.
    if (!item.mediaId.startsWith('yt:')) {
      return;
    }
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
    return Container(
      width: double.infinity,
      color: Theme.of(context).colorScheme.surfaceContainerHighest,
      padding: const EdgeInsets.all(8),
      child: Row(
        children: [
          const Icon(Icons.cloud_off, size: 16),
          const SizedBox(width: 8),
          Expanded(
            child: Text(
              "Showing saved recommendations — couldn't reach the server.",
              style: Theme.of(context).textTheme.bodySmall,
            ),
          ),
        ],
      ),
    );
  }
}

class _ErrorView extends StatelessWidget {
  const _ErrorView({required this.onRetry});

  final VoidCallback onRetry;

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          const Text('Could not load recommendations.'),
          const SizedBox(height: 8),
          FilledButton(onPressed: onRetry, child: const Text('Retry')),
        ],
      ),
    );
  }
}
