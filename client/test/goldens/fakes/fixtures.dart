import 'package:audio_service/audio_service.dart';

import 'package:sunflower/core/api/sunflower_api.dart';

// ---------------------------------------------------------------------------
// Deterministic fixture data for golden tests.
//
// All values are hard-coded so golden images are stable across runs.
// "local:" media IDs avoid any server lookup; hasArt=false keeps the art
// slots occupied by the placeholder icon, also stable across network state.
// ---------------------------------------------------------------------------

abstract final class Fixtures {
  // ─── Songs ──────────────────────────────────────────────────────────────

  static const List<Song> songs = [
    Song(
      mediaId: 'local:aabbcc',
      sourceType: 'local',
      title: 'Sunflower Fields',
      artistName: 'Helios',
      albumTitle: 'Golden Hour',
      hasArt: false,
      albumId: 'album-001',
      durationMs: 212000,
    ),
    Song(
      mediaId: 'local:ddeeff',
      sourceType: 'local',
      title: 'Autumn Drift',
      artistName: 'Maeve',
      albumTitle: 'Slow Burn',
      hasArt: false,
      albumId: 'album-002',
      durationMs: 187000,
    ),
    Song(
      mediaId: 'local:112233',
      sourceType: 'local',
      title: 'Neon Pulse',
      artistName: 'Volta',
      albumTitle: 'Circuits',
      hasArt: false,
      albumId: 'album-003',
      durationMs: 245000,
    ),
    Song(
      mediaId: 'local:445566',
      sourceType: 'local',
      title: 'Still Water',
      artistName: 'Fen',
      albumTitle: 'Depths',
      hasArt: false,
      durationMs: 170000,
    ),
  ];

  // ─── Home feed ───────────────────────────────────────────────────────────

  static final HomeFeed homeFeed = HomeFeed(
    stale: false,
    chips: const ['chill', 'focus', 'upbeat'],
    sections: [
      HomeSection(
        id: 'quick_picks',
        title: 'Quick Picks',
        kind: 'quick_picks',
        items: [
          _homeItem('local:aabbcc', 'Sunflower Fields', 'Helios'),
          _homeItem('local:ddeeff', 'Autumn Drift', 'Maeve'),
          _homeItem('local:112233', 'Neon Pulse', 'Volta'),
        ],
      ),
      HomeSection(
        id: 'daily_discover',
        title: 'Daily Discover',
        kind: 'daily_discover',
        items: [
          _homeItem('local:445566', 'Still Water', 'Fen'),
          _homeItem('local:aabbcc', 'Sunflower Fields', 'Helios'),
        ],
      ),
    ],
  );

  static HomeItem _homeItem(String id, String title, String artist) => HomeItem(
        mediaId: id,
        title: title,
        artists: [artist],
        source: 'local',
        durationMs: 200000,
      );

  // ─── Search ──────────────────────────────────────────────────────────────

  static const SearchResults searchResults = SearchResults(
    query: 'sun',
    songs: [
      SearchSong(
        mediaId: 'yt:yt001',
        source: 'yt',
        title: 'Sunflower Fields (Live)',
        artists: ['Helios'],
        durationMs: 212000,
      ),
      SearchSong(
        mediaId: 'yt:yt002',
        source: 'yt',
        title: 'Neon Sun',
        artists: ['Volta'],
        durationMs: 198000,
      ),
    ],
    albums: [
      SearchAlbum(
        browseId: 'MPREb_album001',
        title: 'Golden Hour',
        artists: ['Helios'],
      ),
    ],
    artists: [
      SearchArtist(
        browseId: 'UC_artist001',
        name: 'Helios',
      ),
    ],
  );

  // ─── Stale home feed (cold start) ────────────────────────────────────────

  static HomeFeed get staleFeed => HomeFeed(
        stale: true,
        chips: homeFeed.chips,
        sections: homeFeed.sections,
      );

  // ─── Playlists ───────────────────────────────────────────────────────────

  static const List<Playlist> playlists = [
    Playlist(id: 'pl-001', title: 'Morning Commute', version: 1),
    Playlist(id: 'pl-002', title: 'Late Night Coding', version: 2),
    Playlist(id: 'pl-003', title: 'Weekend Hike', version: 1),
  ];

  static Playlist playlistWithItems(String id) => Playlist(
        id: id,
        title: 'Morning Commute',
        version: 1,
        items: [
          _homeItem('local:aabbcc', 'Sunflower Fields', 'Helios'),
          _homeItem('local:ddeeff', 'Autumn Drift', 'Maeve'),
        ],
      );

  // ─── MediaItem / PlaybackState (for now-playing goldens) ─────────────────

  static final MediaItem mediaItem = MediaItem(
    id: 'local:aabbcc',
    title: 'Sunflower Fields',
    artist: 'Helios',
    album: 'Golden Hour',
    duration: const Duration(seconds: 212),
  );

  static PlaybackState get playbackStatePaused => PlaybackState(
        controls: [MediaControl.play, MediaControl.skipToNext],
        processingState: AudioProcessingState.ready,
        playing: false,
        updatePosition: const Duration(seconds: 42),
        bufferedPosition: const Duration(seconds: 90),
      );

  static PlaybackState get playbackStatePlaying => PlaybackState(
        controls: [MediaControl.pause, MediaControl.skipToNext],
        processingState: AudioProcessingState.ready,
        playing: true,
        updatePosition: const Duration(seconds: 42),
        bufferedPosition: const Duration(seconds: 90),
      );
}
