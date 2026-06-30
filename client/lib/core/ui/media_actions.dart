import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../features/downloads_ui/download_button.dart';
import '../api/sunflower_api.dart';
import '../downloads/downloads_providers.dart';
import '../player/capabilities.dart';
import '../sync/sync_providers.dart';

class LikeButton extends ConsumerStatefulWidget {
  const LikeButton({super.key, required this.mediaId, this.compact = false});

  final String mediaId;
  final bool compact;

  @override
  ConsumerState<LikeButton> createState() => _LikeButtonState();
}

class _LikeButtonState extends ConsumerState<LikeButton> {
  bool _liked = false;

  @override
  Widget build(BuildContext context) {
    return IconButton(
      constraints: widget.compact
          ? const BoxConstraints.tightFor(width: 28, height: 28)
          : null,
      iconSize: widget.compact ? 20 : null,
      padding: widget.compact ? EdgeInsets.zero : null,
      tooltip: _liked ? 'Unlike' : 'Like',
      icon: Icon(_liked ? Icons.favorite : Icons.favorite_border),
      color: _liked ? Theme.of(context).colorScheme.primary : null,
      onPressed: () async {
        final next = !_liked;
        setState(() => _liked = next);
        try {
          await ref.read(bufferedApiProvider).like(widget.mediaId, liked: next);
        } catch (_) {
          if (mounted) setState(() => _liked = !next);
        }
      },
    );
  }
}

class MediaDownloadButton extends ConsumerWidget {
  const MediaDownloadButton({
    super.key,
    required this.mediaId,
    required this.title,
    this.compact = false,
  });

  final String mediaId;
  final String title;
  final bool compact;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    if (!PlayerCapabilities.offlineDownloads) {
      return IconButton(
        tooltip: 'Downloads not supported',
        icon: const Icon(Icons.cloud_off_outlined),
        onPressed: null,
      );
    }
    return IconButton(
      iconSize: compact ? 20 : null,
      tooltip: 'Download',
      icon: const Icon(Icons.download_outlined),
      onPressed: () => _download(context, ref),
    );
  }

  Future<void> _download(BuildContext context, WidgetRef ref) async {
    final api = ref.read(sunflowerApiProvider);
    final mgr = ref.read(downloadManagerProvider);
    await mgr.start();
    var url = mediaId.startsWith('yt:')
        ? (await api.resolveStream(mediaId)).streamUrl
        : trackStreamUrl(api, mediaId);
    if (url.isEmpty) {
      if (context.mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('Could not resolve this track')),
        );
      }
      return;
    }
    await mgr.enqueueTrack(mediaId: mediaId, title: title, streamUrl: url);
    if (context.mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Downloading $title')),
      );
    }
  }
}

class MediaOverflowMenu extends ConsumerWidget {
  const MediaOverflowMenu({
    super.key,
    required this.mediaId,
    required this.title,
    this.enableAddToPlaylist = true,
  });

  final String mediaId;
  final String title;
  final bool enableAddToPlaylist;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return PopupMenuButton<_MediaAction>(
      tooltip: 'More',
      icon: const Icon(Icons.more_vert),
      onSelected: (action) async {
        switch (action) {
          case _MediaAction.like:
            await ref.read(bufferedApiProvider).like(mediaId, liked: true);
            break;
          case _MediaAction.download:
            await MediaDownloadButton(mediaId: mediaId, title: title)
                ._download(context, ref);
            break;
          case _MediaAction.addToPlaylist:
            await _showPlaylistSheet(context, ref);
            break;
        }
      },
      itemBuilder: (context) => [
        const PopupMenuItem(
          value: _MediaAction.like,
          child: ListTile(
            leading: Icon(Icons.favorite_border),
            title: Text('Like'),
            contentPadding: EdgeInsets.zero,
          ),
        ),
        const PopupMenuItem(
          value: _MediaAction.download,
          child: ListTile(
            leading: Icon(Icons.download_outlined),
            title: Text('Download'),
            contentPadding: EdgeInsets.zero,
          ),
        ),
        PopupMenuItem(
          value: _MediaAction.addToPlaylist,
          enabled: enableAddToPlaylist && mediaId.startsWith('local:'),
          child: const ListTile(
            leading: Icon(Icons.playlist_add),
            title: Text('Add to playlist'),
            contentPadding: EdgeInsets.zero,
          ),
        ),
      ],
    );
  }

  Future<void> _showPlaylistSheet(BuildContext context, WidgetRef ref) async {
    final api = ref.read(sunflowerApiProvider);
    final playlists = await api.listPlaylists();
    if (!context.mounted) return;
    await showModalBottomSheet<void>(
      context: context,
      showDragHandle: true,
      builder: (context) {
        if (playlists.isEmpty) {
          return const SizedBox(
            height: 160,
            child: Center(child: Text('No playlists yet')),
          );
        }
        return ListView(
          shrinkWrap: true,
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(20, 0, 20, 8),
              child: Text(
                'Add to playlist',
                style: Theme.of(context).textTheme.titleMedium,
              ),
            ),
            for (final playlist in playlists)
              ListTile(
                leading: const Icon(Icons.queue_music),
                title: Text(playlist.title),
                onTap: () async {
                  await ref
                      .read(bufferedApiProvider)
                      .addPlaylistItem(playlist.id, mediaId);
                  if (context.mounted) Navigator.pop(context);
                },
              ),
          ],
        );
      },
    );
  }
}

enum _MediaAction { like, download, addToPlaylist }
