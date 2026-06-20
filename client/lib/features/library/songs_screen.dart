import 'package:cached_network_image/cached_network_image.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/api/sunflower_api.dart';
import '../../core/player/player_bootstrap.dart';
import '../player_ui/mini_player.dart';

// ---------------------------------------------------------------------------
// Data providers
// ---------------------------------------------------------------------------

final _songsProvider = FutureProvider<List<Song>>((ref) async {
  return ref.watch(sunflowerApiProvider).listSongs();
});

// ---------------------------------------------------------------------------
// Screen
// ---------------------------------------------------------------------------

class SongsScreen extends ConsumerStatefulWidget {
  const SongsScreen({super.key});

  @override
  ConsumerState<SongsScreen> createState() => _SongsScreenState();
}

class _SongsScreenState extends ConsumerState<SongsScreen> {
  String _query = '';

  List<Song> _filter(List<Song> songs) {
    if (_query.isEmpty) return songs;
    final q = _query.toLowerCase();
    return songs.where((s) => s.title.toLowerCase().contains(q)).toList();
  }

  Future<void> _play(List<Song> allSongs, int index) async {
    final api = ref.read(sunflowerApiProvider);
    final handler = ref.read(audioHandlerProvider);
    await handler.loadPlaylist(
      allSongs,
      index,
      api.streamUrl,
      api.authHeaders,
    );
  }

  @override
  Widget build(BuildContext context) {
    final songsAsync = ref.watch(_songsProvider);

    return Scaffold(
      appBar: AppBar(
        title: const Text('Library'),
        bottom: PreferredSize(
          preferredSize: const Size.fromHeight(56),
          child: Padding(
            padding: const EdgeInsets.fromLTRB(12, 0, 12, 8),
            child: TextField(
              decoration: InputDecoration(
                hintText: 'Search songs…',
                prefixIcon: const Icon(Icons.search),
                border: OutlineInputBorder(
                  borderRadius: BorderRadius.circular(8),
                ),
                isDense: true,
                contentPadding: const EdgeInsets.symmetric(vertical: 8),
              ),
              onChanged: (v) => setState(() => _query = v),
            ),
          ),
        ),
      ),
      body: Column(
        children: [
          Expanded(
            child: songsAsync.when(
              data: (songs) {
                final visible = _filter(songs);
                if (visible.isEmpty) {
                  return Center(
                    child: Text(_query.isEmpty
                        ? 'No songs found.\nScan a music folder from the server.'
                        : 'No songs match "$_query".'),
                  );
                }
                return RefreshIndicator(
                  onRefresh: () => ref.refresh(_songsProvider.future),
                  child: ListView.builder(
                    itemCount: visible.length,
                    itemBuilder: (context, i) {
                      final song = visible[i];
                      return _SongTile(
                        song: song,
                        api: ref.read(sunflowerApiProvider),
                        onTap: () => _play(songs, songs.indexOf(song)),
                      );
                    },
                  ),
                );
              },
              loading: () => const Center(child: CircularProgressIndicator()),
              error: (e, _) => Center(
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Text('Failed to load songs:\n$e'),
                    const SizedBox(height: 16),
                    FilledButton(
                      onPressed: () => ref.refresh(_songsProvider),
                      child: const Text('Retry'),
                    ),
                  ],
                ),
              ),
            ),
          ),
          // Mini-player docked at the bottom.
          const MiniPlayer(),
        ],
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Song tile
// ---------------------------------------------------------------------------

class _SongTile extends StatelessWidget {
  const _SongTile({
    required this.song,
    required this.api,
    required this.onTap,
  });

  final Song song;
  final SunflowerApi api;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    Widget leading;
    if (song.hasArt && song.albumId != null) {
      leading = ClipRRect(
        borderRadius: BorderRadius.circular(4),
        child: CachedNetworkImage(
          imageUrl: api.artUrl(song.albumId!, size: 256),
          httpHeaders: api.authHeaders,
          width: 48,
          height: 48,
          fit: BoxFit.cover,
          placeholder: (_, __) => _artPlaceholder(),
          errorWidget: (_, __, ___) => _artPlaceholder(),
        ),
      );
    } else {
      leading = _artPlaceholder();
    }

    return ListTile(
      leading: leading,
      title: Text(song.title, maxLines: 1, overflow: TextOverflow.ellipsis),
      subtitle: Text(
        [song.artistName, song.albumTitle]
            .where((s) => s.isNotEmpty)
            .join(' · '),
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
      ),
      onTap: onTap,
    );
  }

  Widget _artPlaceholder() => Container(
        width: 48,
        height: 48,
        decoration: BoxDecoration(
          color: Colors.grey[800],
          borderRadius: BorderRadius.circular(4),
        ),
        child: const Icon(Icons.music_note, size: 24),
      );
}
