// Golden tests — library screens: songs, playlists, playlist detail.
//
// Baseline: flutter test --update-goldens test/goldens/library_golden_test.dart
// Compare:  flutter test test/goldens/library_golden_test.dart

import 'package:sunflower/core/api/sunflower_api.dart'
    show sunflowerApiProvider;
import 'package:sunflower/features/library/playlist_detail_screen.dart';
import 'package:sunflower/features/library/playlists_screen.dart';
import 'package:sunflower/features/library/songs_screen.dart';

import 'helpers/golden_harness.dart';

void main() {
  // ══════════════════════════════════════════════════════════════════════════
  // Songs screen — populated / empty / error
  // ══════════════════════════════════════════════════════════════════════════

  // ── 1. songs_screen_populated ─────────────────────────────────────────────
  // Default base overrides supply Fixtures.songs (4 tracks). No extra override.
  testGoldenWidget(
    'songs screen — populated',
    'library/songs_screen_populated',
    const SongsScreen(),
  );

  // ── 2. songs_screen_empty ─────────────────────────────────────────────────
  // FakeSunflowerApi(songs: []) resolves listSongs immediately with an empty
  // list; SongsScreen shows an empty ListView.
  testGoldenWidget(
    'songs screen — empty',
    'library/songs_screen_empty',
    const SongsScreen(),
    overrides: [
      sunflowerApiProvider.overrideWithValue(
        FakeSunflowerApi(songs: const []),
      ),
    ],
  );

  // ── 3. songs_screen_error ─────────────────────────────────────────────────
  // FutureProvider resolves to error state; screen shows error message.
  testGoldenWidget(
    'songs screen — error',
    'library/songs_screen_error',
    const SongsScreen(),
    overrides: [
      sunflowerApiProvider.overrideWithValue(
        FakeSunflowerApi(songsError: Exception('Server down')),
      ),
    ],
  );

  // ══════════════════════════════════════════════════════════════════════════
  // Playlists screen — populated / empty
  // ══════════════════════════════════════════════════════════════════════════

  // ── 4. playlists_populated ────────────────────────────────────────────────
  // Base overrides supply Fixtures.playlists (3 items).
  testGoldenWidget(
    'playlists screen — populated',
    'library/playlists_populated',
    const PlaylistsScreen(),
  );

  // ── 5. playlists_empty ────────────────────────────────────────────────────
  // Override listPlaylists to return []; screen shows "No playlists yet".
  testGoldenWidget(
    'playlists screen — empty',
    'library/playlists_empty',
    const PlaylistsScreen(),
    overrides: [
      playlistsProvider.overrideWith((_) async => const <Playlist>[]),
    ],
  );

  // ══════════════════════════════════════════════════════════════════════════
  // Playlist detail screen — populated / empty
  // ══════════════════════════════════════════════════════════════════════════

  // ── 6. playlist_detail_populated ─────────────────────────────────────────
  // Base overrides supply Fixtures.playlistWithItems('pl-001') (2 tracks).
  testGoldenWidget(
    'playlist detail — populated',
    'library/playlist_detail_populated',
    const PlaylistDetailScreen(playlistId: 'pl-001'),
  );

  // ── 7. playlist_detail_empty ──────────────────────────────────────────────
  // Override getPlaylist to return a Playlist with no items; screen shows
  // "This playlist is empty".
  testGoldenWidget(
    'playlist detail — empty',
    'library/playlist_detail_empty',
    const PlaylistDetailScreen(playlistId: 'pl-001'),
    overrides: [
      sunflowerApiProvider.overrideWithValue(
        FakeSunflowerApi(
          playlist: const Playlist(
            id: 'pl-001',
            title: 'Empty Playlist',
            version: 1,
          ),
        ),
      ),
    ],
  );
}
