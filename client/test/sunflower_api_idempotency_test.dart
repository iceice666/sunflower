import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sunflower/core/api/sunflower_api.dart';
import 'package:sunflower/core/auth/register_device.dart';
import 'package:sunflower/core/auth/token_store.dart';
import 'package:sunflower/core/sync/sync_providers.dart';

void main() {
  test('path builders encode dynamic path segments', () {
    final api = SunflowerApi(
      baseUrl: 'http://sunflower.test',
      token: 'test-token',
    );

    expect(
      api.streamUrl('local:abc/def'),
      'http://sunflower.test/api/v1/library/songs/local%3Aabc%2Fdef/stream',
    );
    expect(
      api.artUrl('local:album/def', size: 256),
      'http://sunflower.test/api/v1/library/albums/local%3Aalbum%2Fdef/art?size=256',
    );
  });

  test('startQueue sends a UUIDv7 Idempotency-Key and legacy body', () async {
    final request = Completer<HttpRequest>();
    final body = Completer<String>();
    final server = await _oneShotServer(
      request,
      bodyText: body,
      responseBody:
          '{"queue_id":"queue-1","seed_kind":"song","version":1,"items":[]}',
    );
    addTearDown(server.close);

    final api = SunflowerApi(
      baseUrl: 'http://127.0.0.1:${server.port}',
      token: 'test-token',
    );

    final queue = await api.startQueue(
      seedKind: 'song',
      seedId: 'yt:abc',
      title: 'Radio',
    );

    final seen = await request.future;
    expect(queue.queueId, 'queue-1');
    expect(seen.method, 'POST');
    expect(seen.uri.path, '/api/v1/queue/start');
    expectUuidV7(seen.headers.value('idempotency-key'));
    expect(jsonDecode(await body.future), {
      'seed_kind': 'song',
      'seed_id': 'yt:abc',
      'title': 'Radio',
    });
  });

  test('resolveStream sends a UUIDv7 Idempotency-Key and recovery context',
      () async {
    final request = Completer<HttpRequest>();
    final body = Completer<String>();
    final server = await _oneShotServer(
      request,
      bodyText: body,
      responseBody:
          '{"media_id":"yt:abc","source":"proxy","stream_url":"https://stream.example/audio","stream_expires_at":"2026-07-01T00:00:00Z"}',
    );
    addTearDown(server.close);

    final api = SunflowerApi(
      baseUrl: 'http://127.0.0.1:${server.port}',
      token: 'test-token',
    );

    final stream = await api.resolveStream(
      'yt:abc',
      proxy: true,
      audioQuality: 'high',
      reason: 'http_403',
    );

    final seen = await request.future;
    expect(stream.mediaId, 'yt:abc');
    expect(stream.source, 'proxy');
    expect(seen.method, 'POST');
    expect(seen.uri.path, '/api/v1/streams/resolve');
    expectUuidV7(seen.headers.value('idempotency-key'));
    expect(jsonDecode(await body.future), {
      'media_id': 'yt:abc',
      'proxy': true,
      'audio_quality': 'high',
      'reason': 'http_403',
    });
  });

  test('logImpressions sends a UUIDv7 Idempotency-Key', () async {
    final request = Completer<HttpRequest>();
    final server = await _oneShotServer(request);
    addTearDown(server.close);

    final api = SunflowerApi(
      baseUrl: 'http://127.0.0.1:${server.port}',
      token: 'test-token',
    );

    await api.logImpressions([
      {
        'section_id': 'quick_picks',
        'source': 'local',
        'seed_id': '',
        'media_id': 'local:one',
        'position': 0,
      },
    ]);

    final seen = await request.future;
    expect(seen.method, 'POST');
    expect(seen.uri.path, '/api/v1/impressions');
    expectUuidV7(seen.headers.value('idempotency-key'));
  });

  test('toggleLike sends a UUIDv7 Idempotency-Key and occurred_at', () async {
    final request = Completer<HttpRequest>();
    final body = Completer<String>();
    final server = await _oneShotServer(request, bodyText: body);
    addTearDown(server.close);

    final api = SunflowerApi(
      baseUrl: 'http://127.0.0.1:${server.port}',
      token: 'test-token',
    );

    final liked = await api.toggleLike(
      'local:one',
      true,
      occurredAt: DateTime.utc(2026, 7, 1, 1, 2, 3, 456),
    );

    final seen = await request.future;
    expect(liked, isTrue);
    expect(seen.method, 'POST');
    expect(seen.uri.path, '/api/v1/likes');
    expectUuidV7(seen.headers.value('idempotency-key'));
    expect(jsonDecode(await body.future), {
      'media_id': 'local:one',
      'liked': true,
      'occurred_at': '2026-07-01T01:02:03.456Z',
    });
  });

  test('recommendationApiProvider uses standalone recommendation URL',
      () async {
    final request = Completer<HttpRequest>();
    final server = await _oneShotServer(
      request,
      responseBody: '{"sections":[],"chips":["remote"],"stale":false}',
    );
    addTearDown(server.close);

    final container = ProviderContainer(
      overrides: [
        tokenProvider.overrideWith((ref) async => 'test-token'),
        serverUrlProvider.overrideWith((ref) async => 'http://main.invalid'),
        recommendationServerUrlProvider.overrideWith(
          (ref) async => 'http://127.0.0.1:${server.port}',
        ),
      ],
    );
    addTearDown(container.dispose);
    await container.read(tokenProvider.future);
    await container.read(serverUrlProvider.future);
    await container.read(recommendationServerUrlProvider.future);

    final feed = await container.read(recommendationApiProvider).home();

    final seen = await request.future;
    expect(feed.chips, ['remote']);
    expect(seen.method, 'GET');
    expect(seen.uri.path, '/api/v1/home');
    expect(seen.headers.value('authorization'), 'Bearer test-token');
  });

  test(
      'recommendationFeedbackClientProvider uses standalone recommendation URL',
      () async {
    final request = Completer<HttpRequest>();
    final body = Completer<String>();
    final server = await _oneShotServer(request, bodyText: body);
    addTearDown(server.close);
    const eventId = '018f3f27-0000-7000-8000-000000000201';

    final container = ProviderContainer(
      overrides: [
        tokenProvider.overrideWith((ref) async => 'test-token'),
        serverUrlProvider.overrideWith((ref) async => 'http://main.invalid'),
        recommendationServerUrlProvider.overrideWith(
          (ref) async => 'http://127.0.0.1:${server.port}',
        ),
      ],
    );
    addTearDown(container.dispose);
    await container.read(tokenProvider.future);
    await container.read(serverUrlProvider.future);
    await container.read(recommendationServerUrlProvider.future);

    final key = await container
        .read(recommendationFeedbackClientProvider)
        .playbackEvent(
          kind: 'play',
          mediaId: 'yt:feedback',
          queueId: '',
          totalPlayedMs: 30000,
          durationMs: 120000,
          occurredAt: DateTime.utc(2026, 7, 1, 1, 2, 3),
          idempotencyKey: eventId,
        );

    final seen = await request.future;
    expect(key, eventId);
    expect(seen.method, 'POST');
    expect(seen.uri.path, '/api/v1/events');
    expect(seen.headers.value('authorization'), 'Bearer test-token');
    expect(seen.headers.value('idempotency-key'), eventId);
    expect(jsonDecode(await body.future), {
      'events': [
        {
          'event_id': eventId,
          'kind': 'play',
          'media_id': 'yt:feedback',
          'queue_id': '',
          'total_played_ms': 30000,
          'duration_ms': 120000,
          'occurred_at': '2026-07-01T01:02:03.000Z',
        },
      ],
    });
  });

  test('recommendationFeedbackClientProvider falls back to main server URL',
      () async {
    final request = Completer<HttpRequest>();
    final server = await _oneShotServer(request);
    addTearDown(server.close);
    const eventId = '018f3f27-0000-7000-8000-000000000202';

    final container = ProviderContainer(
      overrides: [
        tokenProvider.overrideWith((ref) async => 'test-token'),
        serverUrlProvider.overrideWith(
          (ref) async => 'http://127.0.0.1:${server.port}',
        ),
        recommendationServerUrlProvider.overrideWith((ref) async => null),
      ],
    );
    addTearDown(container.dispose);
    await container.read(tokenProvider.future);
    await container.read(serverUrlProvider.future);
    await container.read(recommendationServerUrlProvider.future);

    final key = await container.read(recommendationFeedbackClientProvider).like(
          'local:fallback',
          liked: true,
          occurredAt: DateTime.utc(2026, 7, 1, 1, 2, 3),
          idempotencyKey: eventId,
        );

    final seen = await request.future;
    expect(key, eventId);
    expect(seen.method, 'POST');
    expect(seen.uri.path, '/api/v1/likes');
    expect(seen.headers.value('authorization'), 'Bearer test-token');
    expect(seen.headers.value('idempotency-key'), eventId);
  });

  test('deleteDownload sends a UUIDv7 Idempotency-Key', () async {
    final request = Completer<HttpRequest>();
    final server = await _oneShotServer(request);
    addTearDown(server.close);

    final api = SunflowerApi(
      baseUrl: 'http://127.0.0.1:${server.port}',
      token: 'test-token',
    );

    await api.deleteDownload(
      '018f3f27-0000-7000-8000-000000000001',
      'yt:abc/def',
    );

    final seen = await request.future;
    expect(seen.method, 'DELETE');
    expect(
      seen.uri.toString(),
      '/api/v1/devices/018f3f27-0000-7000-8000-000000000001/downloads/yt%3Aabc%2Fdef',
    );
    expectUuidV7(seen.headers.value('idempotency-key'));
  });

  test('registerDevice sends a UUIDv7 Idempotency-Key', () async {
    final request = Completer<HttpRequest>();
    final server = await _oneShotServer(
      request,
      responseBody:
          '{"token":"sf_dev_test","device_id":"018f3f27-0000-7000-8000-000000000001"}',
    );
    addTearDown(server.close);

    final result = await registerDevice(
      'http://127.0.0.1:${server.port}',
      pairingCode: '123456',
    );

    expect(result.token, 'sf_dev_test');
    final seen = await request.future;
    expect(seen.method, 'POST');
    expect(seen.uri.path, '/api/v1/auth/register-device');
    expectUuidV7(seen.headers.value('idempotency-key'));
  });

  test('youtubeCredentialStatus parses status response', () async {
    final request = Completer<HttpRequest>();
    final server = await _oneShotServer(
      request,
      responseBody:
          '{"status":"ok","checked_at":"2026-07-01T00:00:00Z","detail":"probe ok"}',
    );
    addTearDown(server.close);

    final api = SunflowerApi(
      baseUrl: 'http://127.0.0.1:${server.port}',
      token: 'test-token',
    );

    final status = await api.youtubeCredentialStatus();

    final seen = await request.future;
    expect(seen.method, 'GET');
    expect(seen.uri.path, '/api/v1/cookies/youtube/status');
    expect(status.status, 'ok');
    expect(status.checkedAt, DateTime.utc(2026, 7, 1));
    expect(status.detail, 'probe ok');
  });

  test('uploadYoutubeCredentials sends token and UUIDv7 Idempotency-Key',
      () async {
    final request = Completer<HttpRequest>();
    final body = Completer<String>();
    final server = await _oneShotServer(request, bodyText: body);
    addTearDown(server.close);

    final api = SunflowerApi(
      baseUrl: 'http://127.0.0.1:${server.port}',
      token: 'test-token',
    );

    await api.uploadYoutubeCredentials(
      innertubeToken: 'po_token=po\nvisitor_data=visitor',
    );

    final seen = await request.future;
    expect(seen.method, 'POST');
    expect(seen.uri.path, '/api/v1/cookies/youtube');
    expect(seen.headers.value('authorization'), 'Bearer test-token');
    expectUuidV7(seen.headers.value('idempotency-key'));
    expect(jsonDecode(await body.future), {
      'cookies': '',
      'innertube_token': 'po_token=po\nvisitor_data=visitor',
    });
  });
}

void expectUuidV7(String? value) {
  expect(value, isNotNull);
  expect(
    value,
    matches(
      RegExp(
        r'^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$',
      ),
    ),
  );
}

Future<HttpServer> _oneShotServer(
  Completer<HttpRequest> request, {
  String responseBody = '{}',
  Completer<String>? bodyText,
}) async {
  final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
  server.listen((incoming) async {
    request.complete(incoming);
    final payload = await utf8.decoder.bind(incoming).join();
    bodyText?.complete(payload);
    incoming.response
      ..statusCode = HttpStatus.ok
      ..headers.contentType = ContentType.json
      ..write(responseBody);
    await incoming.response.close();
  });
  return server;
}
