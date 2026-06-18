import 'dart:async';

import 'package:audio_service/audio_service.dart';

import 'now_playing_socket.dart';

/// Applies incoming server → client commands (pause/play/skip) from the
/// now-playing socket to the audio handler — the remote-control path (M8).
class CommandHandler {
  CommandHandler({
    required NowPlayingSocket socket,
    required BaseAudioHandler handler,
  })  : _socket = socket,
        _handler = handler;

  final NowPlayingSocket _socket;
  final BaseAudioHandler _handler;
  StreamSubscription? _sub;

  /// Subscribes to command frames and dispatches them to the handler.
  void start() {
    _sub = _socket.messages.listen((m) {
      if (m['type'] != 'command') return;
      switch (m['command'] as String?) {
        case 'pause':
          _handler.pause();
        case 'play':
          _handler.play();
        case 'skip_next':
          _handler.skipToNext();
        case 'skip_prev':
          _handler.skipToPrevious();
      }
    });
  }

  Future<void> dispose() async {
    await _sub?.cancel();
  }
}
