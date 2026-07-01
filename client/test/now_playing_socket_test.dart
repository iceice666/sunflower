import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:stream_channel/stream_channel.dart';
import 'package:sunflower/core/ws/now_playing_socket.dart';
import 'package:web_socket_channel/web_socket_channel.dart';

void main() {
  test('connect requests the legacy now-playing subprotocol', () {
    Uri? connectedUri;
    Iterable<String>? connectedProtocols;

    final socket = NowPlayingSocket(
      baseUrl: 'https://sunflower.test/base',
      token: 'sf_dev_token',
      connect: (uri, {protocols}) {
        connectedUri = uri;
        connectedProtocols = protocols;
        return _FakeWebSocketChannel();
      },
    );

    socket.connect();

    expect(
      connectedUri.toString(),
      'wss://sunflower.test/base/api/v1/ws/now-playing?token=sf_dev_token',
    );
    expect(connectedProtocols, [nowPlayingSubprotocol]);
  });
}

class _FakeWebSocketChannel
    with StreamChannelMixin<dynamic>
    implements WebSocketChannel {
  _FakeWebSocketChannel()
      : _incoming = StreamController<dynamic>(),
        _sink = _FakeWebSocketSink();

  final StreamController<dynamic> _incoming;
  final _FakeWebSocketSink _sink;

  @override
  String? get protocol => nowPlayingSubprotocol;

  @override
  int? get closeCode => null;

  @override
  String? get closeReason => null;

  @override
  Future<void> get ready => Future.value();

  @override
  Stream get stream => _incoming.stream;

  @override
  WebSocketSink get sink => _sink;
}

class _FakeWebSocketSink implements WebSocketSink {
  final _controller = StreamController<dynamic>();

  @override
  Future get done => _controller.done;

  @override
  void add(Object? event) {
    _controller.add(event);
  }

  @override
  void addError(Object error, [StackTrace? stackTrace]) {
    _controller.addError(error, stackTrace);
  }

  @override
  Future addStream(Stream stream) => _controller.addStream(stream);

  @override
  Future close([int? closeCode, String? closeReason]) {
    return _controller.close();
  }
}
