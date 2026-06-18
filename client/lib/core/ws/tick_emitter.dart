import 'dart:async';

import 'package:audio_service/audio_service.dart';

import 'now_playing_socket.dart';

/// Emits ~1 Hz position ticks over the now-playing socket while audio is
/// playing, and goes silent when paused (no noisy no-op ticks — M8 acceptance).
/// A transition frame is sent immediately whenever the current MediaItem changes
/// so observers update without waiting for the next tick.
class TickEmitter {
  TickEmitter({
    required NowPlayingSocket socket,
    required Stream<MediaItem?> mediaItemStream,
    required Stream<PlaybackState> playbackStateStream,
    required Duration Function() position,
    Duration interval = const Duration(seconds: 1),
  })  : _socket = socket,
        _mediaItems = mediaItemStream,
        _states = playbackStateStream,
        _position = position,
        _interval = interval;

  final NowPlayingSocket _socket;
  final Stream<MediaItem?> _mediaItems;
  final Stream<PlaybackState> _states;
  final Duration Function() _position;
  final Duration _interval;

  Timer? _timer;
  StreamSubscription? _itemSub;
  StreamSubscription? _stateSub;
  MediaItem? _current;
  bool _playing = false;

  /// Begins observing player streams and ticking.
  void start() {
    _itemSub = _mediaItems.listen((item) {
      _current = item;
      _sendFrame('transition'); // immediate update on track change
    });
    _stateSub = _states.listen((s) {
      final wasPlaying = _playing;
      _playing = s.playing;
      if (_playing != wasPlaying) {
        _sendFrame('state'); // play/pause edge
        _updateTimer();
      }
    });
    _updateTimer();
  }

  void _updateTimer() {
    _timer?.cancel();
    if (_playing) {
      _timer = Timer.periodic(_interval, (_) => _sendFrame('tick'));
    } else {
      _timer = null; // silent while paused
    }
  }

  void _sendFrame(String type) {
    final item = _current;
    if (item == null) return;
    _socket.send({
      'type': type,
      'media_id': item.id,
      'title': item.title,
      if (item.artist != null) 'artist': item.artist,
      'position_ms': _position().inMilliseconds,
      if (item.duration != null) 'duration_ms': item.duration!.inMilliseconds,
      'is_playing': _playing,
    });
  }

  Future<void> dispose() async {
    _timer?.cancel();
    await _itemSub?.cancel();
    await _stateSub?.cancel();
  }
}
