import 'package:audio_service/audio_service.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/player/player_bootstrap.dart';
import '../../core/ui/artwork_tile.dart';
import '../../core/ui/media_actions.dart';
import '../../core/ui/sunflower_theme.dart';
import 'queue_panel.dart';

/// Full-screen now-playing view. Opened when the user taps the MiniPlayer.
class NowPlayingScreen extends ConsumerWidget {
  const NowPlayingScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final item = ref.watch(currentMediaItemProvider).valueOrNull;
    final state = ref.watch(playbackStateProvider).valueOrNull;
    final handler = ref.read(audioHandlerProvider);
    final playing = state?.playing ?? false;
    final artUrl = item?.artUri?.scheme.startsWith('http') == true
        ? item!.artUri.toString()
        : null;

    return Scaffold(
      appBar: AppBar(
        title: const Text('Now Playing'),
        leading: const CloseButton(),
      ),
      body: Stack(
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(24, 16, 24, 118),
            child: Column(
              children: [
                Expanded(
                  child: Center(
                    child: ArtworkTile(
                      imageUrl: artUrl,
                      size: 292,
                      radius: 18,
                      icon: Icons.music_note,
                    ),
                  ),
                ),
                const SizedBox(height: 18),
                Text(
                  item?.title ?? '-',
                  style: Theme.of(context).textTheme.headlineSmall?.copyWith(
                        fontWeight: FontWeight.w900,
                      ),
                  textAlign: TextAlign.center,
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                ),
                const SizedBox(height: 4),
                Text(
                  item?.artist ?? '',
                  style: Theme.of(context).textTheme.bodyMedium,
                  textAlign: TextAlign.center,
                ),
                const SizedBox(height: 12),
                if (item != null)
                  Row(
                    mainAxisAlignment: MainAxisAlignment.center,
                    children: [
                      LikeButton(mediaId: item.id),
                      MediaDownloadButton(mediaId: item.id, title: item.title),
                      MediaOverflowMenu(mediaId: item.id, title: item.title),
                    ],
                  ),
                const SizedBox(height: 12),
                _SeekBar(
                  state: state,
                  handler: handler,
                  duration: item?.duration,
                ),
                const SizedBox(height: 18),
                Row(
                  mainAxisAlignment: MainAxisAlignment.spaceEvenly,
                  children: [
                    IconButton(
                      iconSize: 42,
                      icon: const Icon(Icons.skip_previous),
                      onPressed: handler.skipToPrevious,
                    ),
                    IconButton.filled(
                      iconSize: 42,
                      icon: Icon(playing ? Icons.pause : Icons.play_arrow),
                      onPressed: playing ? handler.pause : handler.play,
                    ),
                    IconButton(
                      iconSize: 42,
                      icon: const Icon(Icons.skip_next),
                      onPressed: handler.skipToNext,
                    ),
                  ],
                ),
                const SizedBox(height: 24),
              ],
            ),
          ),
          DraggableScrollableSheet(
            initialChildSize: 0.16,
            minChildSize: 0.12,
            maxChildSize: 0.54,
            builder: (context, scrollController) {
              return Container(
                decoration: const BoxDecoration(
                  color: SunflowerColors.surface,
                  borderRadius: BorderRadius.vertical(top: Radius.circular(22)),
                  border:
                      Border(top: BorderSide(color: SunflowerColors.outline)),
                ),
                child: Column(
                  children: [
                    const SizedBox(height: 8),
                    Container(
                      width: 36,
                      height: 4,
                      decoration: BoxDecoration(
                        color: Colors.white24,
                        borderRadius: BorderRadius.circular(99),
                      ),
                    ),
                    Padding(
                      padding: const EdgeInsets.fromLTRB(20, 12, 20, 8),
                      child: Row(
                        children: [
                          Text(
                            'Up next',
                            style: Theme.of(context)
                                .textTheme
                                .titleMedium
                                ?.copyWith(fontWeight: FontWeight.w800),
                          ),
                          const Spacer(),
                          const Icon(Icons.queue_music, size: 20),
                        ],
                      ),
                    ),
                    Expanded(
                      child: PrimaryScrollController(
                        controller: scrollController,
                        child: const QueuePanel(),
                      ),
                    ),
                  ],
                ),
              );
            },
          ),
        ],
      ),
    );
  }
}

class _SeekBar extends StatelessWidget {
  const _SeekBar({required this.state, required this.handler, this.duration});

  final PlaybackState? state;
  final BaseAudioHandler handler;
  final Duration? duration;

  @override
  Widget build(BuildContext context) {
    final position = state?.position ?? Duration.zero;
    final buffered = state?.bufferedPosition ?? Duration.zero;
    final total = duration ?? buffered;
    final maxMs = total.inMilliseconds <= 0 ? 1 : total.inMilliseconds;
    final valueMs = position.inMilliseconds.clamp(0, maxMs).toDouble();

    return Column(
      children: [
        Slider(
          value: valueMs,
          max: maxMs.toDouble(),
          onChanged: (v) => handler.seek(Duration(milliseconds: v.round())),
        ),
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 4),
          child: Row(
            children: [
              Text(_fmt(position),
                  style: Theme.of(context).textTheme.bodySmall),
              const Spacer(),
              Text(_fmt(total), style: Theme.of(context).textTheme.bodySmall),
            ],
          ),
        ),
      ],
    );
  }

  String _fmt(Duration d) {
    final minutes = d.inMinutes.remainder(60).toString();
    final seconds = d.inSeconds.remainder(60).toString().padLeft(2, '0');
    return '$minutes:$seconds';
  }
}
