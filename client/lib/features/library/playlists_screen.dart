import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/api/sunflower_api.dart';
import 'playlist_detail_screen.dart';

final playlistsProvider = FutureProvider.autoDispose<List<Playlist>>((
  ref,
) async {
  return ref.watch(sunflowerApiProvider).listPlaylists();
});

/// Lists the user's playlists with a create-playlist action (M5 playlist CRUD).
class PlaylistsScreen extends ConsumerWidget {
  const PlaylistsScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final playlistsAsync = ref.watch(playlistsProvider);

    return Scaffold(
      appBar: AppBar(title: const Text('Playlists')),
      floatingActionButton: FloatingActionButton(
        onPressed: () => _createPlaylist(context, ref),
        child: const Icon(Icons.add),
      ),
      body: playlistsAsync.when(
        data: (playlists) {
          if (playlists.isEmpty) {
            return const Center(child: Text('No playlists yet'));
          }
          return ListView.builder(
            itemCount: playlists.length,
            itemBuilder: (context, i) {
              final pl = playlists[i];
              return ListTile(
                leading: const Icon(Icons.queue_music),
                title: Text(pl.title),
                onTap: () => Navigator.of(context).push(
                  MaterialPageRoute(
                    builder: (_) => PlaylistDetailScreen(playlistId: pl.id),
                  ),
                ),
              );
            },
          );
        },
        loading: () => const Center(child: CircularProgressIndicator()),
        error: (e, _) => const Center(child: Text('Could not load playlists')),
      ),
    );
  }

  Future<void> _createPlaylist(BuildContext context, WidgetRef ref) async {
    final controller = TextEditingController();
    final title = await showDialog<String>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('New Playlist'),
        content: TextField(
          controller: controller,
          autofocus: true,
          decoration: const InputDecoration(hintText: 'Playlist name'),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, controller.text.trim()),
            child: const Text('Create'),
          ),
        ],
      ),
    );
    if (title == null || title.isEmpty) return;
    await ref.read(sunflowerApiProvider).createPlaylist(title);
    ref.invalidate(playlistsProvider);
  }
}
