import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/api/sunflower_api.dart';
import '../../core/downloads/downloads_providers.dart';
import '../../core/player/capabilities.dart';

final _playlistDetailProvider =
    FutureProvider.autoDispose.family<Playlist, String>((ref, id) async {
  return ref.watch(sunflowerApiProvider).getPlaylist(id);
});

/// Shows a playlist's tracks (M5). Tapping a track is a future tap-to-play hook
/// (queue start by media id, like the home feed).
class PlaylistDetailScreen extends ConsumerWidget {
  const PlaylistDetailScreen({super.key, required this.playlistId});

  final String playlistId;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final detailAsync = ref.watch(_playlistDetailProvider(playlistId));

    return Scaffold(
      appBar: AppBar(
        title: Text(detailAsync.valueOrNull?.title ?? 'Playlist'),
        actions: [
          if (PlayerCapabilities.offlineDownloads)
            IconButton(
              icon: const Icon(Icons.download_outlined),
              tooltip: 'Download for offline',
              onPressed: () async {
                final mgr = ref.read(downloadManagerProvider);
                await mgr.start();
                await mgr.enqueuePlaylist(playlistId);
                if (context.mounted) {
                  ScaffoldMessenger.of(context).showSnackBar(
                    const SnackBar(content: Text('Downloading playlist…')),
                  );
                }
              },
            ),
        ],
      ),
      body: detailAsync.when(
        data: (pl) {
          if (pl.items.isEmpty) {
            return const Center(child: Text('This playlist is empty'));
          }
          return ListView.builder(
            itemCount: pl.items.length,
            itemBuilder: (context, i) {
              final item = pl.items[i];
              return ListTile(
                leading: const Icon(Icons.music_note),
                title: Text(
                  item.title,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                ),
                subtitle: item.artists.isEmpty
                    ? null
                    : Text(
                        item.artists.join(', '),
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                      ),
              );
            },
          );
        },
        loading: () => const Center(child: CircularProgressIndicator()),
        error: (e, _) => const Center(child: Text('Could not load playlist')),
      ),
    );
  }
}
