import 'package:flutter/material.dart';

import '../downloads_ui/downloads_screen.dart';
import '../settings/settings_screen.dart';
import 'playlists_screen.dart';
import 'songs_screen.dart';

class LibraryScreen extends StatefulWidget {
  const LibraryScreen({super.key});

  @override
  State<LibraryScreen> createState() => _LibraryScreenState();
}

class _LibraryScreenState extends State<LibraryScreen> {
  _LibraryTab _tab = _LibraryTab.songs;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Library'),
        actions: [
          IconButton(
            tooltip: 'Settings',
            icon: const Icon(Icons.settings_outlined),
            onPressed: () => Navigator.of(context).push(
              MaterialPageRoute(builder: (_) => const SettingsScreen()),
            ),
          ),
        ],
      ),
      body: Column(
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 4, 16, 12),
            child: SizedBox(
              width: double.infinity,
              child: SegmentedButton<_LibraryTab>(
                showSelectedIcon: false,
                segments: const [
                  ButtonSegment(
                    value: _LibraryTab.songs,
                    icon: Icon(Icons.music_note),
                    label: Text('Songs'),
                  ),
                  ButtonSegment(
                    value: _LibraryTab.playlists,
                    icon: Icon(Icons.queue_music),
                    label: Text('Playlists'),
                  ),
                  ButtonSegment(
                    value: _LibraryTab.downloads,
                    icon: Icon(Icons.download_done),
                    label: Text('Downloads'),
                  ),
                ],
                selected: {_tab},
                onSelectionChanged: (next) => setState(() => _tab = next.first),
              ),
            ),
          ),
          Expanded(
            child: IndexedStack(
              index: _tab.index,
              children: const [
                SongsPane(),
                PlaylistsPane(),
                DownloadsPane(),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

enum _LibraryTab { songs, playlists, downloads }
