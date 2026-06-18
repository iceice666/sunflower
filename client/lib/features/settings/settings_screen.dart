import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/ws/ws_providers.dart';
import 'crossfade_setting.dart';
import 'sync_status_widget.dart';

/// Settings surface (M8): crossfade config + sync status. Reading this screen
/// also activates the now-playing socket (tick emission + remote control) via
/// [nowPlayingProvider].
class SettingsScreen extends ConsumerWidget {
  const SettingsScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    // Activate the now-playing socket while the app is in the authed shell.
    ref.watch(nowPlayingProvider);

    return Scaffold(
      appBar: AppBar(title: const Text('Settings')),
      body: ListView(
        children: const [
          SyncStatusWidget(),
          Divider(),
          CrossfadeSetting(),
        ],
      ),
    );
  }
}
