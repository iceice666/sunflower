import 'package:audio_service/audio_service.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/player/player_bootstrap.dart';
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

    final playing = stateAsync.valueOrNull?.playing ?? false;
    final handler = ref.read(audioHandlerProvider);

    return GestureDetector(
      onTap: () => Navigator.of(context).push(
        MaterialPageRoute(builder: (_) => const NowPlayingScreen()),
      ),
      child: Container(
        height: 64,
        color: Theme.of(context).colorScheme.surfaceContainerHighest,
        padding: const EdgeInsets.symmetric(horizontal: 12),
        child: Row(
          children: [
            Expanded(
              child: Column(
                mainAxisAlignment: MainAxisAlignment.center,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    item.title,
                    style: Theme.of(context).textTheme.bodyMedium?.copyWith(
                          fontWeight: FontWeight.bold,
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
              icon: const Icon(Icons.skip_previous),
              onPressed: handler.skipToPrevious,
            ),
            IconButton(
              icon: Icon(playing ? Icons.pause : Icons.play_arrow),
              onPressed: playing ? handler.pause : handler.play,
            ),
            IconButton(
              icon: const Icon(Icons.skip_next),
              onPressed: handler.skipToNext,
            ),
          ],
        ),
      ),
    );
  }
}
