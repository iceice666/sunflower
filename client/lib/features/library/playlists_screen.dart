import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/api/sunflower_api.dart';
import '../../core/sync/sync_providers.dart';
import '../../core/ui/empty_state.dart';
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
        onPressed: () => createPlaylist(context, ref),
        child: const Icon(Icons.add),
      ),
      body: PlaylistsPane(playlistsAsync: playlistsAsync),
    );
  }
}

class PlaylistsPane extends ConsumerWidget {
  const PlaylistsPane({super.key, this.playlistsAsync});

  final AsyncValue<List<Playlist>>? playlistsAsync;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final AsyncValue<List<Playlist>> async =
        playlistsAsync ?? ref.watch(playlistsProvider);
    return async.when(
      data: (playlists) {
        if (playlists.isEmpty) {
          return EmptyState(
            icon: Icons.queue_music_outlined,
            title: 'No playlists yet',
            message: 'Create a playlist for downloaded sets, radios, or moods.',
            action: FilledButton.icon(
              onPressed: () => createPlaylist(context, ref),
              icon: const Icon(Icons.add),
              label: const Text('New playlist'),
            ),
          );
        }
        return ListView(
          padding: const EdgeInsets.only(bottom: 16),
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 8, 16, 8),
              child: FilledButton.icon(
                onPressed: () => createPlaylist(context, ref),
                icon: const Icon(Icons.add),
                label: const Text('New playlist'),
              ),
            ),
            for (final pl in playlists)
              ListTile(
                leading: const Icon(Icons.queue_music),
                title: Text(pl.title),
                subtitle: Text('Version ${pl.version}'),
                onTap: () => Navigator.of(context).push(
                  MaterialPageRoute(
                    builder: (_) => PlaylistDetailScreen(playlistId: pl.id),
                  ),
                ),
              ),
          ],
        );
      },
      loading: () => const Center(child: CircularProgressIndicator()),
      error: (e, _) => const EmptyState(
        icon: Icons.error_outline,
        title: 'Could not load playlists',
      ),
    );
  }
}

Future<void> createPlaylist(BuildContext context, WidgetRef ref) async {
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
  controller.dispose();
  if (title == null || title.isEmpty) return;
  await ref.read(bufferedApiProvider).createPlaylist(title);
  ref.invalidate(playlistsProvider);
}
