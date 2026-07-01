import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'core/auth/token_store.dart';
import 'core/recommendations/local_core.dart';
import 'core/ui/sunflower_theme.dart';
import 'core/ws/ws_providers.dart';
import 'features/home/home_screen.dart';
import 'features/library/library_screen.dart';
import 'features/onboarding/server_setup_screen.dart';
import 'features/player_ui/mini_player.dart';
import 'features/search/search_screen.dart';

class SunflowerApp extends ConsumerWidget {
  const SunflowerApp({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final tokenAsync = ref.watch(tokenProvider);

    return MaterialApp(
      title: 'Sunflower',
      theme: sunflowerTheme(),
      darkTheme: sunflowerTheme(),
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

/// Bottom-navigation shell for the authenticated app.
class MainShell extends ConsumerStatefulWidget {
  const MainShell({super.key});

  @override
  ConsumerState<MainShell> createState() => _MainShellState();
}

class _MainShellState extends ConsumerState<MainShell> {
  int _index = 0;

  static const _tabs = [
    HomeScreen(),
    SearchScreen(),
    LibraryScreen(),
  ];

  @override
  Widget build(BuildContext context) {
    // Activate the now-playing socket for the whole authed session (tick
    // emission + remote control).
    ref.watch(nowPlayingProvider);
    ref.watch(localFeedbackSyncProvider);

    return Scaffold(
      body: IndexedStack(index: _index, children: _tabs),
      bottomNavigationBar: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          const MiniPlayer(),
          NavigationBar(
            selectedIndex: _index,
            onDestinationSelected: (i) => setState(() => _index = i),
            destinations: const [
              NavigationDestination(
                icon: Icon(Icons.home_outlined),
                selectedIcon: Icon(Icons.home),
                label: 'Home',
              ),
              NavigationDestination(
                icon: Icon(Icons.search),
                selectedIcon: Icon(Icons.manage_search),
                label: 'Search',
              ),
              NavigationDestination(
                icon: Icon(Icons.library_music_outlined),
                selectedIcon: Icon(Icons.library_music),
                label: 'Library',
              ),
            ],
          ),
        ],
      ),
    );
  }
}
