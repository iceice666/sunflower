import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../auth/token_store.dart';
import '../player/player_bootstrap.dart';
import 'command_handler.dart';
import 'now_playing_socket.dart';
import 'tick_emitter.dart';

/// Owns the now-playing socket plus its tick emitter and command handler, wired
/// to the audio handler. Created once credentials are available; reconnects with
/// backoff internally. Disposed when the provider is torn down.
final nowPlayingProvider = Provider<NowPlayingController?>((ref) {
  final baseUrl = ref.watch(serverUrlProvider).valueOrNull;
  final token = ref.watch(tokenProvider).valueOrNull;
  if (baseUrl == null || baseUrl.isEmpty || token == null || token.isEmpty) {
    return null;
  }
  final handler = ref.watch(audioHandlerProvider);

  final socket = NowPlayingSocket(baseUrl: baseUrl, token: token);
  final emitter = TickEmitter(
    socket: socket,
    mediaItemStream: handler.mediaItem.stream,
    playbackStateStream: handler.playbackState.stream,
    position: () => handler.playbackState.value.position,
  );
  final commands = CommandHandler(socket: socket, handler: handler);

  final ctrl = NowPlayingController(socket, emitter, commands);
  ctrl.start();
  ref.onDispose(ctrl.dispose);
  return ctrl;
});

/// Bundles the socket lifecycle so a single provider can start/stop everything.
class NowPlayingController {
  NowPlayingController(this._socket, this._emitter, this._commands);

  final NowPlayingSocket _socket;
  final TickEmitter _emitter;
  final CommandHandler _commands;

  void start() {
    _socket.connect();
    _emitter.start();
    _commands.start();
  }

  Future<void> dispose() async {
    await _emitter.dispose();
    await _commands.dispose();
    await _socket.close();
  }
}
