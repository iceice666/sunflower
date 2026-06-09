import 'package:audio_service/audio_service.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'app.dart';
import 'core/player/player_bootstrap.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();

  // Register the audio handler with the OS media session.
  // The handler is stored globally and surfaced via audioHandlerProvider.
  final handler = await AudioService.init(
    builder: createAudioHandler,
    config: const AudioServiceConfig(
      androidNotificationChannelId: 'com.iceice666.sunflower.audio',
      androidNotificationChannelName: 'Sunflower',
      androidNotificationOngoing: true,
      androidStopForegroundOnPause: true,
    ),
  );

  runApp(
    ProviderScope(
      overrides: [audioHandlerProvider.overrideWithValue(handler)],
      child: const SunflowerApp(),
    ),
  );
}
