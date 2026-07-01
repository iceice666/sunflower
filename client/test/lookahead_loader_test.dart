import 'dart:convert';
import 'dart:io';

import 'package:drift/native.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sunflower/core/api/sunflower_api.dart';
import 'package:sunflower/core/db/database.dart';
import 'package:sunflower/core/player/lookahead_loader.dart';

void main() {
  late SunflowerDatabase db;

  setUp(() {
    db = SunflowerDatabase.forTesting(NativeDatabase.memory());
  });

  tearDown(() async {
    await db.close();
  });

  test('buffers and caches resolved lookahead stream fields', () async {
    final server = await _nextServer({
      'queue_id': 'queue-1',
      'position': 0,
      'current': {
        'media_id': 'yt:current',
        'title': 'Current',
        'source': 'youtube',
        'stream_url': 'https://stream.example/current',
        'stream_expires_at': '2026-07-01T00:00:00Z',
        'mime_type': 'audio/webm',
      },
      'lookahead': [
        {
          'media_id': 'yt:next',
          'title': 'Next',
          'artists': ['Artist'],
          'duration_ms': 180000,
          'source': 'youtube',
          'stream_url': 'https://stream.example/next',
          'stream_expires_at': '2026-07-01T01:00:00Z',
          'mime_type': 'audio/webm',
        },
      ],
      'queue_version': 7,
      'has_more': false,
    });
    addTearDown(() => server.close(force: true));

    final api = SunflowerApi(
      baseUrl: 'http://127.0.0.1:${server.port}',
      token: 'test-token',
    );
    final loader = LookaheadLoader(api: api, db: db, queueId: 'queue-1');

    final current = await loader.start(0);

    expect(current?.streamUrl, 'https://stream.example/current');
    expect(loader.buffered.single.resolvedStream?.streamUrl,
        'https://stream.example/next');

    final rows = await _cachedRowsEventually(db, 'queue-1', 2);
    expect(rows[0].mediaId, 'yt:current');
    expect(rows[0].streamUrl, 'https://stream.example/current');
    expect(rows[1].mediaId, 'yt:next');
    expect(rows[1].source, 'youtube');
    expect(rows[1].streamUrl, 'https://stream.example/next');
    expect(rows[1].streamExpiresAt?.toUtc(), DateTime.utc(2026, 7, 1, 1));
    expect(rows[1].mimeType, 'audio/webm');
  });
}

Future<HttpServer> _nextServer(Map<String, Object?> body) async {
  final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
  server.listen((request) async {
    expect(request.method, 'GET');
    expect(request.uri.path, '/api/v1/next');
    request.response
      ..statusCode = HttpStatus.ok
      ..headers.contentType = ContentType.json
      ..write(jsonEncode(body));
    await request.response.close();
  });
  return server;
}

Future<List<LookaheadCacheData>> _cachedRowsEventually(
  SunflowerDatabase db,
  String queueId,
  int expectedCount,
) async {
  for (var attempt = 0; attempt < 20; attempt++) {
    final rows = await db.cachedLookahead(queueId);
    if (rows.length == expectedCount) return rows;
    await Future<void>.delayed(const Duration(milliseconds: 10));
  }
  return db.cachedLookahead(queueId);
}
