import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'core/auth/token_store.dart';
import 'core/ws/ws_providers.dart';
import 'features/downloads_ui/downloads_screen.dart';
import 'features/home/home_screen.dart';
import 'features/library/playlists_screen.dart';
import 'features/library/songs_screen.dart';
import 'features/onboarding/server_setup_screen.dart';
import 'features/settings/settings_screen.dart';

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
            token != null ? const MainShell() : const ServerSetupScreen(),
        loading: () =>
            const Scaffold(body: Center(child: CircularProgressIndicator())),
        error: (e, _) => const ServerSetupScreen(),
      ),
    );
  }
}

/// Bottom-navigation shell for the authenticated app: Home (recommendations),
/// Songs (local library), and Playlists. Added in M5 when the home feed gave the
/// app more than one primary surface.
class MainShell extends ConsumerStatefulWidget {
  const MainShell({super.key});

  @override
  ConsumerState<MainShell> createState() => _MainShellState();
}

class _MainShellState extends ConsumerState<MainShell> {
  int _index = 0;

  static const _tabs = [
    HomeScreen(),
    SongsScreen(),
    PlaylistsScreen(),
    DownloadsScreen(),
    SettingsScreen(),
  ];

  @override
  Widget build(BuildContext context) {
    // Activate the now-playing socket for the whole authed session (tick
    // emission + remote control).
    ref.watch(nowPlayingProvider);

    return Scaffold(
      body: IndexedStack(index: _index, children: _tabs),
      bottomNavigationBar: NavigationBar(
        selectedIndex: _index,
        onDestinationSelected: (i) => setState(() => _index = i),
        destinations: const [
          NavigationDestination(
            icon: Icon(Icons.home_outlined),
            selectedIcon: Icon(Icons.home),
            label: 'Home',
          ),
          NavigationDestination(
            icon: Icon(Icons.library_music_outlined),
            selectedIcon: Icon(Icons.library_music),
            label: 'Songs',
          ),
          NavigationDestination(
            icon: Icon(Icons.queue_music_outlined),
            selectedIcon: Icon(Icons.queue_music),
            label: 'Playlists',
          ),
          NavigationDestination(
            icon: Icon(Icons.download_outlined),
            selectedIcon: Icon(Icons.download),
            label: 'Downloads',
          ),
          NavigationDestination(
            icon: Icon(Icons.settings_outlined),
            selectedIcon: Icon(Icons.settings),
            label: 'Settings',
          ),
        ],
      ),
    );
  }
}
