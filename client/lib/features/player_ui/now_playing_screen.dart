import 'package:audio_service/audio_service.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/player/player_bootstrap.dart';

/// Full-screen now-playing view. Opened when the user taps the MiniPlayer.
class NowPlayingScreen extends ConsumerWidget {
  const NowPlayingScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final item = ref.watch(currentMediaItemProvider).valueOrNull;
    final state = ref.watch(playbackStateProvider).valueOrNull;
    final handler = ref.read(audioHandlerProvider);
    final playing = state?.playing ?? false;

    return Scaffold(
      appBar: AppBar(
        title: const Text('Now Playing'),
        leading: const CloseButton(),
      ),
      body: Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          children: [
            // Art placeholder (artUri is file:// once downloaded in M2+)
            Expanded(
              child: Center(
                child: item?.artUri != null
                    ? Image.network(item!.artUri!.toString())
                    : Container(
                        width: 260,
                        height: 260,
                        decoration: BoxDecoration(
                          color: Colors.grey[800],
                          borderRadius: BorderRadius.circular(12),
                        ),
                        child: const Icon(Icons.music_note, size: 80),
                      ),
              ),
            ),
            const SizedBox(height: 24),
            Text(
              item?.title ?? '—',
              style: Theme.of(context).textTheme.headlineSmall,
              textAlign: TextAlign.center,
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
            ),
            Text(
              item?.artist ?? '',
              style: Theme.of(context).textTheme.bodyMedium,
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: 32),
            // Seek bar
            _SeekBar(state: state, handler: handler),
            const SizedBox(height: 24),
            // Transport controls
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceEvenly,
              children: [
                IconButton(
                  iconSize: 40,
                  icon: const Icon(Icons.skip_previous),
                  onPressed: handler.skipToPrevious,
                ),
                IconButton(
                  iconSize: 64,
                  icon: Icon(playing ? Icons.pause_circle : Icons.play_circle),
                  onPressed: playing ? handler.pause : handler.play,
                ),
                IconButton(
                  iconSize: 40,
                  icon: const Icon(Icons.skip_next),
                  onPressed: handler.skipToNext,
                ),
              ],
            ),
            const SizedBox(height: 24),
          ],
        ),
      ),
    );
  }
}

class _SeekBar extends StatelessWidget {
  const _SeekBar({required this.state, required this.handler});

  final PlaybackState? state;
  final BaseAudioHandler handler;

  @override
  Widget build(BuildContext context) {
    final position = state?.position ?? Duration.zero;
    final buffered = state?.bufferedPosition ?? Duration.zero;

    // Duration comes from audio_service after just_audio resolves it.
    // In M2 the server sends duration_ms = 0 (dhowden/tag limitation), so
    // the slider is indeterminate until just_audio loads the stream.
    return Slider(
      value: position.inMilliseconds.toDouble(),
      max: buffered.inMilliseconds.toDouble().clamp(
            position.inMilliseconds.toDouble(),
            double.infinity,
          ),
      onChanged: (v) => handler.seek(Duration(milliseconds: v.round())),
    );
  }
}
