import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/api/sunflower_api.dart';
import '../../core/db/database_provider.dart';
import '../../core/player/player_bootstrap.dart';
import '../../core/ui/artwork_tile.dart';
import '../../core/ui/empty_state.dart';
import '../../core/ui/track_row.dart';

class SearchScreen extends ConsumerStatefulWidget {
  const SearchScreen({super.key});

  @override
  ConsumerState<SearchScreen> createState() => _SearchScreenState();
}

class _SearchScreenState extends ConsumerState<SearchScreen> {
  final _controller = TextEditingController();
  Timer? _debounce;
  String _query = '';
  Future<SearchResults>? _future;

  @override
  void dispose() {
    _debounce?.cancel();
    _controller.dispose();
    super.dispose();
  }

  void _onChanged(String value) {
    _debounce?.cancel();
    final query = value.trim();
    setState(() => _query = query);
    if (query.length < 2) {
      setState(() => _future = null);
      return;
    }
    _debounce = Timer(const Duration(milliseconds: 350), () {
      if (!mounted) return;
      setState(() {
        _future = _search(query);
      });
    });
  }

  void _submit(String value) {
    _debounce?.cancel();
    final query = value.trim();
    setState(() {
      _query = query;
      _future = query.length < 2 ? null : _search(query);
    });
  }

  Future<SearchResults> _search(String query) {
    return Future.microtask(() => ref.read(sunflowerApiProvider).search(query));
  }

  Future<void> _play(SearchSong song) async {
    final api = ref.read(sunflowerApiProvider);
    final db = ref.read(databaseProvider);
    final handler = ref.read(audioHandlerProvider);
    final queue = await api.startQueue(
      seedKind: 'song',
      seedId: song.mediaId,
      title: song.title,
    );
    await handler.startQueue(
      api: api,
      db: db,
      queueId: queue.queueId,
      authHeaders: api.authHeaders,
    );
  }

  @override
  Widget build(BuildContext context) {
    final future = _future;
    return Scaffold(
      appBar: AppBar(title: const Text('Search')),
      body: Column(
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 4, 16, 12),
            child: TextField(
              controller: _controller,
              textInputAction: TextInputAction.search,
              decoration: InputDecoration(
                hintText: 'Songs, albums, artists',
                prefixIcon: const Icon(Icons.search),
                suffixIcon: _query.isEmpty
                    ? null
                    : IconButton(
                        icon: const Icon(Icons.close),
                        onPressed: () {
                          _controller.clear();
                          _onChanged('');
                        },
                      ),
              ),
              onChanged: _onChanged,
              onSubmitted: _submit,
            ),
          ),
          Expanded(
            child: future == null
                ? const EmptyState(
                    icon: Icons.manage_search,
                    title: 'Search YouTube Music',
                    message:
                        'Type at least two characters to find playable songs.',
                  )
                : FutureBuilder<SearchResults>(
                    future: future,
                    builder: (context, snapshot) {
                      if (snapshot.connectionState != ConnectionState.done) {
                        return const Center(child: CircularProgressIndicator());
                      }
                      if (snapshot.hasError) {
                        return EmptyState(
                          icon: Icons.cloud_off,
                          title: 'Search unavailable',
                          message:
                              'The server could not reach YouTube Music for this query.',
                          action: FilledButton(
                            onPressed: () => _submit(_query),
                            child: const Text('Retry'),
                          ),
                        );
                      }
                      final results = snapshot.data;
                      if (results == null || results.isEmpty) {
                        return EmptyState(
                          icon: Icons.search_off,
                          title: 'No results',
                          message: 'Nothing matched "$_query".',
                        );
                      }
                      return _Results(results: results, onPlay: _play);
                    },
                  ),
          ),
        ],
      ),
    );
  }
}

class _Results extends StatelessWidget {
  const _Results({required this.results, required this.onPlay});

  final SearchResults results;
  final Future<void> Function(SearchSong song) onPlay;

  @override
  Widget build(BuildContext context) {
    return ListView(
      padding: const EdgeInsets.only(bottom: 18),
      children: [
        if (results.songs.isNotEmpty) ...[
          const _Header('Songs'),
          for (final song in results.songs)
            TrackRow(
              mediaId: song.mediaId,
              title: song.title,
              subtitle: song.artists.join(', '),
              thumbnailUrl: song.thumbnailUrl,
              onTap: () => onPlay(song),
              leadingIcon: Icons.play_arrow,
              showActions: true,
            ),
        ],
        if (results.albums.isNotEmpty) ...[
          const _Header('Albums'),
          for (final album in results.albums)
            _DisabledResultRow(
              title: album.title,
              subtitle: album.artists.join(', '),
              imageUrl: album.thumbnailUrl,
              icon: Icons.album,
            ),
        ],
        if (results.artists.isNotEmpty) ...[
          const _Header('Artists'),
          for (final artist in results.artists)
            _DisabledResultRow(
              title: artist.name,
              subtitle: 'Artist page coming soon',
              imageUrl: artist.thumbnailUrl,
              icon: Icons.person,
              circular: true,
            ),
        ],
      ],
    );
  }
}

class _Header extends StatelessWidget {
  const _Header(this.text);

  final String text;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 18, 16, 6),
      child: Text(
        text,
        style: Theme.of(context)
            .textTheme
            .titleMedium
            ?.copyWith(fontWeight: FontWeight.w800),
      ),
    );
  }
}

class _DisabledResultRow extends StatelessWidget {
  const _DisabledResultRow({
    required this.title,
    required this.subtitle,
    required this.imageUrl,
    required this.icon,
    this.circular = false,
  });

  final String title;
  final String subtitle;
  final String? imageUrl;
  final IconData icon;
  final bool circular;

  @override
  Widget build(BuildContext context) {
    return ListTile(
      enabled: false,
      leading: ArtworkTile(
        imageUrl: imageUrl,
        icon: icon,
        shape: circular ? BoxShape.circle : BoxShape.rectangle,
      ),
      title: Text(title, maxLines: 1, overflow: TextOverflow.ellipsis),
      subtitle: Text(
        subtitle.isEmpty ? 'Coming soon' : '$subtitle · Coming soon',
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
      ),
    );
  }
}
