import 'dart:async';

import 'package:just_audio/just_audio.dart';

/// Optional crossfade helper (M8): ramps one AudioPlayer's volume down while a
/// secondary player ramps up, so a track transition has no audible gap. Behind
/// the crossfade setting; when disabled the queue plays gaplessly as before.
///
/// This is a focused volume-ramp utility, not a full dual-deck engine — the
/// owning handler swaps which player is "active" after the fade completes.
class CrossfadePlayer {
  CrossfadePlayer({Duration tick = const Duration(milliseconds: 50)})
      : _tick = tick;

  final Duration _tick;

  /// Linearly fades [from] out to 0 and [to] in to 1 over [duration]. Starts
  /// [to] playing at volume 0 first. Completes when the ramp finishes.
  Future<void> crossfade({
    required AudioPlayer from,
    required AudioPlayer to,
    required Duration duration,
  }) async {
    await to.setVolume(0);
    await to.play();

    final steps = (duration.inMilliseconds / _tick.inMilliseconds)
        .clamp(1, 10000)
        .floor();
    for (var i = 1; i <= steps; i++) {
      final t = i / steps;
      await from.setVolume((1 - t).clamp(0, 1));
      await to.setVolume(t.clamp(0, 1));
      await Future<void>.delayed(_tick);
    }
    await from.pause();
    await from.setVolume(1); // reset for reuse
  }
}
