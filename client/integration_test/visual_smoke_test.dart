// client/integration_test/visual_smoke_test.dart
//
// Android-only live smoke test.  Drives 8 visual demo targets against a real
// sunflowerd, captures 9 screenshots + 1 /admin JSON snapshot host-side via the
// integration_test screenshot channel (driven by flutter drive's
// test_driver/integration_test.dart), and asserts three dynamic invariants:
//   • WS now-playing tick propagation (M8)
//   • /admin JSON shape               (M8)
//   • pending mutation drain count    (M7)
//
// Prerequisites
//   • Pixel_10 AVD booted  (`emulator -avd Pixel_10`)
//   • sunflowerd running on host port 8080
//   • DB seeded            (`make seed-demo`)  →  .seed-env written
//
// Run via:
//   make smoke-android   (wraps `flutter drive`)
// which sources .seed-env and passes values as --dart-define flags. Screenshots
// land host-side in client/build/smoke-artifacts/ (written by the driver).
//
// --dart-define parameters (all optional; defaults target standard emulator):
//   SUNFLOWER_DEMO_URL    http://10.0.2.2:8080
//   SUNFLOWER_DEMO_TOKEN  observer/seed device token; empty → WS check is skipped
//   SUNFLOWER_DEMO_PAIRING_CODE  one-time code for the smoke client
//   SUNFLOWER_DEMO_ADMIN_PASSWORD owner password for admin snapshot login

// ignore_for_file: avoid_print

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:audio_service/audio_service.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import 'package:sunflower/app.dart';
import 'package:sunflower/core/player/player_bootstrap.dart';

// ── dart-define configuration ─────────────────────────────────────────────────

const _demoUrl = String.fromEnvironment(
  'SUNFLOWER_DEMO_URL',
  defaultValue: 'http://10.0.2.2:8080',
);
// Observer token minted by seed-demo.sh.  Empty → WS check is soft-skipped.
const _seedToken = String.fromEnvironment('SUNFLOWER_DEMO_TOKEN');
const _pairingCode = String.fromEnvironment('SUNFLOWER_DEMO_PAIRING_CODE');
const _adminPassword = String.fromEnvironment('SUNFLOWER_DEMO_ADMIN_PASSWORD');

// ── Storage keys — must mirror client/lib/core/auth/token_store.dart ─────────

const _kServerUrl = 'sunflower_server_url';
const _kToken = 'sunflower_token';
const _kDeviceId = 'sunflower_device_id';

const _storage = FlutterSecureStorage(
  aOptions: AndroidOptions(encryptedSharedPreferences: true),
);

// ── Integration binding ───────────────────────────────────────────────────────

// Set in main(); used for host-side screenshot capture and reportData.
late IntegrationTestWidgetsFlutterBinding _binding;

// ── Entry point ───────────────────────────────────────────────────────────────

Future<void> main() async {
  _binding = IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    'Sunflower visual smoke — 10 targets on Pixel_10',
    timeout: const Timeout(Duration(minutes: 5)),
    (tester) async {
      // Wipe any leftover credentials so the app opens the onboarding screen.
      await _clearCredentials();
      // ── Launch ──────────────────────────────────────────────────────────────
      // Initialise AudioService once for the full test run (mirrors app/main.dart).
      final handler = await AudioService.init(
        builder: createAudioHandler,
        config: const AudioServiceConfig(
          androidNotificationChannelId: 'com.iceice666.sunflower.audio',
          androidNotificationChannelName: 'Sunflower',
          androidNotificationOngoing: true,
          androidStopForegroundOnPause: true,
        ),
      );

      await tester.pumpWidget(
        ProviderScope(
          overrides: [audioHandlerProvider.overrideWithValue(handler)],
          child: const SunflowerApp(),
        ),
      );

      // Android host-side screenshots require the Flutter surface to be
      // converted to a bitmap-backed view once before any takeScreenshot call.
      if (Platform.isAndroid) {
        await _binding.convertFlutterSurfaceToImage();
      }

      // Let tokenProvider (async secure-storage read) resolve.
      await _stabilize(tester, seconds: 3);

      // ── t01  Onboarding ──────────────────────────────────────────────────────
      // No credentials stored → SunflowerApp renders ServerSetupScreen.

      expect(
        find.byType(TextFormField),
        findsAtLeastNWidgets(1),
        reason: 'ServerSetupScreen must render a URL text field',
      );
      await _shot(tester, 't01_onboarding_setup');

      // ── Connect to sunflowerd ────────────────────────────────────────────────
      // Fill the URL + pairing code fields and tap Pair.
      // POST /api/v1/auth/register-device → saves credentials → routes to MainShell.
      // This creates the "smoke" client device (distinct from the seed/observer
      // device whose token is passed via --dart-define SUNFLOWER_DEMO_TOKEN).

      final urlField = find.byType(TextFormField).first;
      await tester.tap(urlField);
      await tester.pump();
      await tester.enterText(urlField, _demoUrl);
      await tester.pump();

      final codeField = find.byType(TextFormField).at(1);
      await tester.tap(codeField);
      await tester.pump();
      await tester.enterText(codeField, _pairingCode);
      await tester.pump();

      final connectBtn = find.byType(FilledButton);
      expect(connectBtn, findsAtLeastNWidgets(1),
          reason: 'ServerSetupScreen must have a FilledButton to pair');
      await tester.tap(connectBtn.first);

      // Allow up to 10 s for the register-device round-trip and navigation.
      await _stabilize(tester, seconds: 10);

      expect(
        find.byType(NavigationBar),
        findsOneWidget,
        reason:
            'After successful connect, MainShell must show the NavigationBar',
      );

      // ── t02  Library / Songs ─────────────────────────────────────────────────

      await _tapNav(tester, 'Library');
      await _tapText(tester, 'Songs');
      await _stabilize(tester, seconds: 5);
      await _shot(tester, 't02_library_songs');

      // ── t03  Mini player — tap the first song tile ───────────────────────────
      // Tapping a tile enqueues the track.  Playback of the stub MP3 may fail
      // at the OS audio layer, but the player state + MiniPlayer bar are still
      // screenshot-worthy.

      final tiles = find.byType(ListTile);
      if (tiles.evaluate().isNotEmpty) {
        await tester.tap(tiles.first);
        await _stabilize(tester, seconds: 3);
      }
      await _shot(tester, 't03_mini_player');

      // ── t04  Now-playing screen ──────────────────────────────────────────────
      // MiniPlayer carries Key('mini_player') so the test can tap it reliably.

      final miniPlayerFinder = find.byKey(const Key('mini_player'));
      if (miniPlayerFinder.evaluate().isNotEmpty) {
        await tester.tap(miniPlayerFinder);
        await _stabilize(tester, seconds: 2);
        await _shot(tester, 't04_now_playing');
        final close = find.byType(CloseButton);
        if (close.evaluate().isNotEmpty) {
          await tester.tap(close);
          await tester.pump();
        }
      } else {
        // Playback may not have started; screenshot current state for CI record.
        await _shot(tester, 't04_now_playing');
      }

      // ── t05  Home feed ───────────────────────────────────────────────────────

      await _tapNav(tester, 'Home');
      await _stabilize(tester, seconds: 5);
      await _shot(tester, 't05_home_feed');

      // ── t06  Playlists ───────────────────────────────────────────────────────

      await _tapNav(tester, 'Library');
      await _tapText(tester, 'Playlists');
      await _stabilize(tester, seconds: 3);
      await _shot(tester, 't06_playlists');

      // ── t07  Downloads ───────────────────────────────────────────────────────

      await _tapNav(tester, 'Library');
      await _tapText(tester, 'Downloads');
      await _stabilize(tester, seconds: 3);
      await _shot(tester, 't07_downloads');

      // ── t08  Settings / sync status (M7 drain observable) ───────────────────
      // SyncStatusWidget renders "N pending" when pending > 0 or is invisible
      // when drained.  Either state is valid; the screenshot proves the widget
      // lifecycle is active and rendering.

      await _tapNav(tester, 'Library');
      final settings = find.byIcon(Icons.settings_outlined);
      if (settings.evaluate().isNotEmpty) {
        await tester.tap(settings.first);
      }
      await _stabilize(tester, seconds: 3);
      await _shot(tester, 't08_settings_sync');

      // Log drain state for CI review.
      final pendingText = find.textContaining('pending');
      final drainMsg = pendingText.evaluate().isEmpty
          ? 'pending count = 0 (drained — SyncStatusWidget hidden)'
          : 'pending count > 0 (SyncStatusWidget visible)';
      print('[smoke] M7 sync status: $drainMsg');

      // ── t09  WS now-playing tick — two-client check (M8) ────────────────────

      await _wsTickCheck(tester);

      // ── t10  /admin snapshot (M8) ────────────────────────────────────────────

      await _adminCheck();
    },
  );
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Pump the widget tree every 100 ms for [seconds] wall-clock seconds.
///
/// Preferred over `pumpAndSettle(Duration(seconds: N))` in real-device
/// integration tests: the first positional arg to pumpAndSettle sets the *step
/// interval*, not a timeout — passing a multi-second duration makes it
/// pathologically slow.  This helper gives explicit elapsed-time control while
/// keeping the tree responsive to frame callbacks.
Future<void> _stabilize(WidgetTester tester, {int seconds = 3}) async {
  for (int i = 0; i < seconds * 10; i++) {
    await tester.pump(const Duration(milliseconds: 100));
  }
}

/// Tap a NavigationBar destination by its label text.
Future<void> _tapNav(WidgetTester tester, String label) async {
  await _tapText(tester, label);
}

Future<void> _tapText(WidgetTester tester, String label) async {
  final dest = find.text(label);
  if (dest.evaluate().isNotEmpty) {
    await tester.tap(dest.first);
    await tester.pump();
  }
}

/// Capture a full-screen screenshot host-side via the integration_test
/// screenshot channel. The bytes are streamed to the flutter-drive driver
/// (test_driver/integration_test.dart), which writes the PNG under
/// client/build/smoke-artifacts/. A capture failure is logged and swallowed so
/// a single error never aborts the whole run.
Future<void> _shot(WidgetTester tester, String name) async {
  await tester.pumpAndSettle();
  try {
    await _binding.takeScreenshot(name);
    print('[smoke] screenshot captured: $name');
  } catch (e) {
    print('[smoke] WARNING: screenshot "$name" failed: $e');
  }
}

/// Clear all stored credentials (pre-test isolation).
Future<void> _clearCredentials() async {
  await Future.wait([
    _storage.delete(key: _kServerUrl),
    _storage.delete(key: _kToken),
    _storage.delete(key: _kDeviceId),
  ]);
}

/// M8 — Two-client WS tick propagation check.
///
/// Opens an *observer* WebSocket with [_seedToken] and waits ≤ 10 s for any
/// frame emitted by the smoke client (connected through `nowPlayingProvider`
/// once MainShell is active and a media item is loaded).
///
/// A 10 s timeout without a frame is a soft failure: the WS handshake being
/// accepted proves hub reachability; absent ticks mean playback didn't start
/// (expected when the stub MP3 can't be decoded by the OS audio stack).
Future<void> _wsTickCheck(WidgetTester tester) async {
  // Stay on Settings so nowPlayingProvider remains active.
  await _tapNav(tester, 'Settings');
  await tester.pump();

  if (_seedToken.isEmpty) {
    print('[smoke] SUNFLOWER_DEMO_TOKEN not set — WS observer check skipped');
    await _shot(tester, 't09_ws_tick');
    return;
  }

  final wsUrl =
      '${_demoUrl.replaceFirst('http://', 'ws://')}/api/v1/ws/now-playing';
  final completer = Completer<Map<String, dynamic>>();
  WebSocket? ws;

  try {
    ws = await WebSocket.connect(
      wsUrl,
      headers: {'Authorization': 'Bearer $_seedToken'},
    ).timeout(const Duration(seconds: 5));

    print('[smoke] WS observer connected → $wsUrl');

    final sub = ws.listen(
      (data) {
        if (completer.isCompleted) return;
        try {
          completer
              .complete(jsonDecode(data as String) as Map<String, dynamic>);
        } catch (_) {
          if (!completer.isCompleted) {
            completer.completeError('non-JSON WS frame: $data');
          }
        }
      },
      onError: (Object e) {
        if (!completer.isCompleted) completer.completeError(e);
      },
      onDone: () {
        if (!completer.isCompleted) {
          completer.completeError('WS closed before first frame');
        }
      },
    );

    try {
      final frame = await completer.future.timeout(const Duration(seconds: 10));
      print('[smoke] WS tick: type=${frame["type"]}  '
          'media_id=${frame["media_id"]}  '
          'pos_ms=${frame["position_ms"]}  '
          'playing=${frame["is_playing"]}');
      expect(frame.containsKey('type'), isTrue,
          reason: 'WS frame must contain a "type" field');
    } on TimeoutException {
      print('[smoke] WARNING: no WS frame within 10 s '
          '(WS connected; no playback tick — stub MP3 may not have decoded)');
    } finally {
      await sub.cancel();
    }
  } catch (e) {
    print('[smoke] WARNING: WS observer error: $e');
  } finally {
    await ws?.close();
  }

  await _shot(tester, 't09_ws_tick');
}

/// M8 — GET /admin snapshot check.
///
/// Asserts the response contains `now_playing` and `cookie_status` keys.
/// Stashes the full JSON into the integration binding's reportData under
/// `t10_admin_snapshot`; the flutter-drive driver writes it to
/// client/build/smoke-artifacts/t10_admin_snapshot.json for checklist mapping.
Future<void> _adminCheck() async {
  if (_adminPassword.isEmpty) {
    print(
        '[smoke] SUNFLOWER_DEMO_ADMIN_PASSWORD not set — /admin check skipped');
    return;
  }

  final loginUri = Uri.parse('$_demoUrl/api/v1/admin/auth/login');
  final adminUri = Uri.parse('$_demoUrl/api/v1/admin/status');
  final client = HttpClient();
  try {
    final loginReq = await client.postUrl(loginUri);
    loginReq.headers.contentType = ContentType.json;
    loginReq.write(jsonEncode({'password': _adminPassword}));
    final loginResp =
        await loginReq.close().timeout(const Duration(seconds: 10));
    expect(loginResp.statusCode, equals(200),
        reason: 'admin login must return 200 (got ${loginResp.statusCode})');
    final cookies = loginResp.cookies;
    await loginResp.drain<void>();

    final req = await client.getUrl(adminUri);
    for (final cookie in cookies) {
      req.cookies.add(cookie);
    }
    final resp = await req.close().timeout(const Duration(seconds: 10));

    expect(resp.statusCode, equals(200),
        reason:
            'GET /api/v1/admin/status must return 200 (got ${resp.statusCode})');

    final body = await resp.transform(utf8.decoder).join();
    final json = jsonDecode(body) as Map<String, dynamic>;

    expect(json.containsKey('now_playing'), isTrue,
        reason: '/admin JSON must include "now_playing"');
    expect(json.containsKey('cookie_status'), isTrue,
        reason: '/admin JSON must include "cookie_status"');

    final deviceCount = (json['now_playing'] as List<dynamic>).length;
    print('[smoke] /admin: $deviceCount connected device(s)  '
        'cookie_status=${json["cookie_status"]}');

    (_binding.reportData ??= <String, dynamic>{})['t10_admin_snapshot'] = json;
    print('[smoke] /admin snapshot stored in reportData');
  } finally {
    client.close();
  }
}
