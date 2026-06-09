import 'package:audio_service/audio_service.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'sunflower_audio_handler.dart';

/// Holds the singleton [SunflowerAudioHandler] created during [main].
/// Overridden in [ProviderScope] with the instance returned by AudioService.init.
final audioHandlerProvider = Provider<SunflowerAudioHandler>(
  (ref) => throw UnimplementedError('audioHandlerProvider not overridden'),
);

/// Factory passed to AudioService.init in main.dart.
SunflowerAudioHandler createAudioHandler() => SunflowerAudioHandler();

/// Exposes the current [MediaItem] stream for widgets that only need metadata.
final currentMediaItemProvider = StreamProvider<MediaItem?>((ref) {
  return ref.watch(audioHandlerProvider).mediaItem.stream;
});

/// Exposes the current [PlaybackState] stream.
final playbackStateProvider = StreamProvider<PlaybackState>((ref) {
  return ref.watch(audioHandlerProvider).playbackState.stream;
});
