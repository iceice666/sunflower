import 'dart:async';
import 'dart:convert';

import 'package:web_socket_channel/web_socket_channel.dart';

const nowPlayingSubprotocol = 'sunflower.now-playing.v1';

typedef WebSocketConnector = WebSocketChannel Function(
  Uri uri, {
  Iterable<String>? protocols,
});

/// Persistent now-playing WebSocket (M8) with reconnect backoff (5 s → 30 s →
/// 5 min cap). Emits decoded server frames (commands) and accepts outbound
/// client frames (ticks / transitions / state). The tick emitter and command
/// handler sit on top of this transport.
class NowPlayingSocket {
  NowPlayingSocket({
    required String baseUrl,
    required String token,
    WebSocketConnector? connect,
  })  : _wsUrl = _toWsUrl(baseUrl, token),
        _connect = connect ?? WebSocketChannel.connect;

  final Uri _wsUrl;
  final WebSocketConnector _connect;

  static const _backoff = <Duration>[
    Duration(seconds: 5),
    Duration(seconds: 30),
    Duration(minutes: 5), // cap
  ];

  WebSocketChannel? _channel;
  StreamSubscription? _sub;
  Timer? _reconnectTimer;
  int _attempt = 0;
  bool _closed = false;

  final _incoming = StreamController<Map<String, dynamic>>.broadcast();

  /// Stream of decoded server → client frames (command messages).
  Stream<Map<String, dynamic>> get messages => _incoming.stream;

  /// Opens the socket and begins consuming frames. Reconnects automatically on
  /// drop until [close] is called.
  void connect() {
    _closed = false;
    _open();
  }

  void _open() {
    if (_closed) return;
    try {
      final ch = _connect(
        _wsUrl,
        protocols: const [nowPlayingSubprotocol],
      );
      _channel = ch;
      _sub = ch.stream.listen(
        (data) {
          _attempt = 0; // a received frame proves the link is healthy
          _handle(data);
        },
        onDone: _scheduleReconnect,
        onError: (_) => _scheduleReconnect(),
        cancelOnError: true,
      );
    } catch (_) {
      _scheduleReconnect();
    }
  }

  void _handle(dynamic data) {
    if (data is! String) return;
    try {
      final m = jsonDecode(data) as Map<String, dynamic>;
      _incoming.add(m);
    } catch (_) {
      // ignore malformed frames
    }
  }

  /// Sends a client frame (tick / transition / state).
  void send(Map<String, dynamic> frame) {
    final ch = _channel;
    if (ch == null) return;
    try {
      ch.sink.add(jsonEncode(frame));
    } catch (_) {
      // drop sends while reconnecting; the next tick carries fresh state
    }
  }

  void _scheduleReconnect() {
    _sub?.cancel();
    _sub = null;
    _channel = null;
    if (_closed) return;
    final delay = _backoff[_attempt.clamp(0, _backoff.length - 1)];
    if (_attempt < _backoff.length - 1) _attempt++;
    _reconnectTimer?.cancel();
    _reconnectTimer = Timer(delay, _open);
  }

  /// Closes the socket permanently (no further reconnects).
  Future<void> close() async {
    _closed = true;
    _reconnectTimer?.cancel();
    await _sub?.cancel();
    await _channel?.sink.close();
    await _incoming.close();
  }

  /// Builds the ws(s):// URL with the bearer token as a query param (the OS
  /// WebSocket API can't set Authorization headers on web).
  static Uri _toWsUrl(String baseUrl, String token) {
    final base = Uri.parse(baseUrl);
    final scheme = base.scheme == 'https' ? 'wss' : 'ws';
    return base.replace(
      scheme: scheme,
      path: '${base.path}/api/v1/ws/now-playing',
      queryParameters: {'token': token},
    );
  }
}
