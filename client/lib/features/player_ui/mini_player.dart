import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/player/player_bootstrap.dart';
import '../../core/ui/artwork_tile.dart';
import '../../core/ui/sunflower_theme.dart';
import 'now_playing_screen.dart';

/// Slim persistent bar docked at the bottom of the songs list.
/// Visible only when something is playing or paused.
class MiniPlayer extends ConsumerWidget {
  const MiniPlayer({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final mediaItemAsync = ref.watch(currentMediaItemProvider);
    final stateAsync = ref.watch(playbackStateProvider);

    final item = mediaItemAsync.valueOrNull;
    if (item == null) return const SizedBox.shrink();

    final state = stateAsync.valueOrNull;
    final playing = state?.playing ?? false;
    final handler = ref.read(audioHandlerProvider);
    final artUrl = item.artUri?.scheme.startsWith('http') == true
        ? item.artUri.toString()
        : null;
    final positionMs = state?.position.inMilliseconds ?? 0;
    final bufferedMs = state?.bufferedPosition.inMilliseconds ?? 0;
    final progress = bufferedMs <= 0
        ? null
        : (positionMs / bufferedMs).clamp(0.0, 1.0).toDouble();

    return GestureDetector(
      key: const Key('mini_player'),
      onTap: () => Navigator.of(context).push(
        MaterialPageRoute(builder: (_) => const NowPlayingScreen()),
      ),
      child: Container(
        decoration: const BoxDecoration(
          color: SunflowerColors.surface,
          border: Border(top: BorderSide(color: SunflowerColors.outline)),
        ),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            if (progress != null)
              LinearProgressIndicator(value: progress, minHeight: 2),
            SizedBox(
              height: 70,
              child: Row(
                children: [
                  const SizedBox(width: 12),
                  ArtworkTile(imageUrl: artUrl, size: 48, radius: 8),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Column(
                      mainAxisAlignment: MainAxisAlignment.center,
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          item.title,
                          style:
                              Theme.of(context).textTheme.bodyMedium?.copyWith(
                                    fontWeight: FontWeight.w800,
                                  ),
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                        ),
                        if (item.artist != null)
                          Text(
                            item.artist!,
                            style: Theme.of(context).textTheme.bodySmall,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                          ),
                      ],
                    ),
                  ),
                  IconButton(
                    icon: Icon(playing ? Icons.pause : Icons.play_arrow),
                    onPressed: playing ? handler.pause : handler.play,
                  ),
                  IconButton(
                    icon: const Icon(Icons.skip_next),
                    onPressed: handler.skipToNext,
                  ),
                  const SizedBox(width: 4),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}
