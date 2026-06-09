import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'core/auth/token_store.dart';
import 'features/library/songs_screen.dart';
import 'features/onboarding/server_setup_screen.dart';

class SunflowerApp extends ConsumerWidget {
  const SunflowerApp({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final tokenAsync = ref.watch(tokenProvider);

    return MaterialApp(
      title: 'Sunflower',
      theme: ThemeData(
        colorScheme: ColorScheme.fromSeed(
          seedColor: const Color(0xFFFFB300), // sunflower yellow
          brightness: Brightness.dark,
        ),
        useMaterial3: true,
      ),
      darkTheme: ThemeData(
        colorScheme: ColorScheme.fromSeed(
          seedColor: const Color(0xFFFFB300),
          brightness: Brightness.dark,
        ),
        useMaterial3: true,
      ),
      themeMode: ThemeMode.dark,
      home: tokenAsync.when(
        data: (token) =>
            token != null ? const SongsScreen() : const ServerSetupScreen(),
        loading: () => const Scaffold(
          body: Center(child: CircularProgressIndicator()),
        ),
        error: (e, _) => const ServerSetupScreen(),
      ),
    );
  }
}
