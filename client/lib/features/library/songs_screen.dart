import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/api/sunflower_api.dart';
import '../../core/auth/token_store.dart';
import '../../core/bridge/api.dart' as bridge;
import '../../core/db/database_provider.dart';
import '../../core/player/player_bootstrap.dart';
import '../../core/recommendations/local_core.dart';
import '../../core/sync/sync_providers.dart';
import '../../core/ui/empty_state.dart';
import '../../core/ui/track_row.dart';

// ---------------------------------------------------------------------------
// Data providers
// ---------------------------------------------------------------------------

final songsProvider = FutureProvider<List<Song>>((ref) async {
  final localMode = await ref.watch(localModeProvider.future);
  if (localMode) {
    final handle = await ref.watch(localCoreHandleProvider.future);
    if (handle == null) return const [];
    final songs = await bridge.listLocalSongs(
      handle: handle,
      limit: 500,
      offset: 0,
    );
    return songs.where((song) => song.available).map(_songFromLocal).toList();
  }
  return ref.watch(sunflowerApiProvider).listSongs();
});

Song _songFromLocal(bridge.SongDto song) {
  return Song(
    mediaId: song.mediaId,
    sourceType: song.sourceType,
    title: song.title,
    artistName: song.artists.isEmpty ? '' : song.artists.join(', '),
    albumTitle: '',
    hasArt: false,
    albumId: song.albumId,
    durationMs: song.durationMs,
    localPath: song.localPath,
  );
}

// ---------------------------------------------------------------------------
// Screen
// ---------------------------------------------------------------------------

class SongsScreen extends ConsumerStatefulWidget {
  const SongsScreen({super.key});

  @override
  ConsumerState<SongsScreen> createState() => _SongsScreenState();
}

class _SongsScreenState extends ConsumerState<SongsScreen> {
  @override
  Widget build(BuildContext context) {
    return const Scaffold(
      appBar: _SongsAppBar(),
      body: SongsPane(),
    );
  }
}

class SongsPane extends ConsumerStatefulWidget {
  const SongsPane({super.key});

  @override
  ConsumerState<SongsPane> createState() => _SongsPaneState();
}

class _SongsPaneState extends ConsumerState<SongsPane> {
  String _query = '';

  List<Song> _filter(List<Song> songs) {
    if (_query.isEmpty) return songs;
    final q = _query.toLowerCase();
    return songs.where((s) => s.title.toLowerCase().contains(q)).toList();
  }

  Future<void> _play(List<Song> allSongs, int index) async {
    final api = ref.read(sunflowerApiProvider);
    final db = ref.read(databaseProvider);
    final handler = ref.read(audioHandlerProvider);
    final localMode = ref.read(localModeProvider).valueOrNull ?? false;
    final bufferedApi = localMode ? null : ref.read(bufferedApiProvider);
    final localRecommendations =
        await ref.read(localRecommendationRecorderProvider.future);
    await handler.loadPlaylist(
      allSongs,
      index,
      api.streamUrl,
      api.authHeaders,
      db,
      bufferedApi,
      localRecommendations,
    );
  }

  @override
  Widget build(BuildContext context) {
    final songsAsync = ref.watch(songsProvider);

    return Column(
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 8, 16, 12),
          child: TextField(
            decoration: const InputDecoration(
              hintText: 'Search songs…',
              prefixIcon: Icon(Icons.search),
              isDense: true,
            ),
            onChanged: (v) => setState(() => _query = v),
          ),
        ),
        Expanded(
          child: songsAsync.when(
            data: (songs) {
              final visible = _filter(songs);
              if (visible.isEmpty) {
                final localMode =
                    ref.watch(localModeProvider).valueOrNull ?? false;
                return EmptyState(
                  icon: Icons.library_music_outlined,
                  title: _query.isEmpty ? 'No songs found' : 'No matches',
                  message: _query.isEmpty && localMode
                      ? 'Local songs will appear here after playback or import.'
                      : _query.isEmpty
                          ? 'Scan a music folder from the server to fill your library.'
                          : 'No songs match "$_query".',
                );
              }
              return RefreshIndicator(
                onRefresh: () => ref.refresh(songsProvider.future),
                child: ListView.builder(
                  padding: const EdgeInsets.only(bottom: 16),
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
            error: (e, _) => EmptyState(
              icon: Icons.error_outline,
              title: 'Failed to load songs',
              message: '$e',
              action: FilledButton(
                onPressed: () => ref.refresh(songsProvider),
                child: const Text('Retry'),
              ),
            ),
          ),
        ),
      ],
    );
  }
}

class _SongsAppBar extends StatelessWidget implements PreferredSizeWidget {
  const _SongsAppBar();

  @override
  Widget build(BuildContext context) {
    return AppBar(title: const Text('Songs'));
  }

  @override
  Size get preferredSize => const Size.fromHeight(kToolbarHeight);
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
    String? artUrl;
    if (song.hasArt && song.albumId != null) {
      artUrl = api.artUrl(song.albumId!, size: 256);
    }

    return TrackRow(
      mediaId: song.mediaId,
      title: song.title,
      subtitle: [song.artistName, song.albumTitle]
          .where((s) => s.isNotEmpty)
          .join(' · '),
      thumbnailUrl: artUrl,
      httpHeaders: api.authHeaders,
      onTap: onTap,
    );
  }
}
