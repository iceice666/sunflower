// Golden test fakes — provider-boundary stubs; no real network, audio, or DB.
//
// Imported by golden_harness.dart (which re-exports everything), so individual
// screen test files need only one import:
//
//   import '../helpers/golden_harness.dart';

import 'dart:async';

import 'package:audio_service/audio_service.dart';
import 'package:mockito/mockito.dart';

import 'package:sunflower/core/api/api_client.dart';
import 'package:sunflower/core/api/sunflower_api.dart';
import 'package:sunflower/core/player/sunflower_audio_handler.dart';

import 'fixtures.dart';

// ─── FakeSunflowerApi ────────────────────────────────────────────────────────
//
// Two constructors:
//   FakeSunflowerApi(...)  — default constructor; each field falls back to the
//                            Fixtures equivalent when omitted, so the screen
//                            renders in populated state with no extra config.
//   FakeSunflowerApi.loading()  — all fields null → every async call returns a
//                                 Completer that never resolves (loading state).
//
// Field semantics:
//   non-null value  → Future.value(...)     (resolved)
//   null            → Completer().future    (forever loading)
//   *Error field    → Future.error(...)     (error state for that method)

class FakeSunflowerApi extends Fake implements SunflowerApi {
  /// Default: populated state. Omit a field to use the corresponding Fixture.
  FakeSunflowerApi({
    List<Song>? songs,
    this.songsError,
    List<Playlist>? playlists,
    Playlist? playlist,
    HomeFeed? feed,
    this.feedError,
    SearchResults? searchResults,
    this.searchError,
  })  : songs = songs ?? Fixtures.songs,
        playlists = playlists ?? Fixtures.playlists,
        playlist = playlist ?? Fixtures.playlistWithItems('pl-001'),
        feed = feed ?? Fixtures.homeFeed,
        searchResults = searchResults ?? Fixtures.searchResults;

  /// Loading state: every async call returns a Completer that never resolves.
  FakeSunflowerApi.loading()
      : songs = null,
        songsError = null,
        playlists = null,
        playlist = null,
        feed = null,
        feedError = null,
        searchResults = null,
        searchError = null;

  final List<Song>? songs;
  final Object? songsError;
  final List<Playlist>? playlists;
  final Playlist? playlist;
  final HomeFeed? feed;
  final Object? feedError;
  final SearchResults? searchResults;
  final Object? searchError;

  // ─── Library ─────────────────────────────────────────────────────────────

  @override
  Future<List<Song>> listSongs({int limit = 100, int offset = 0}) {
    if (songsError != null) return Future.error(songsError!);
    final s = songs;
    return s != null ? Future.value(s) : Completer<List<Song>>().future;
  }

  // ─── Home feed ───────────────────────────────────────────────────────────

  @override
  Future<HomeFeed> home({
    bool hideExplicit = false,
    bool hideVideo = false,
    bool hideShorts = false,
  }) {
    if (feedError != null) return Future.error(feedError!);
    final f = feed;
    return f != null ? Future.value(f) : Completer<HomeFeed>().future;
  }

  @override
  Future<SearchResults> search(String query, {int limit = 20}) {
    if (searchError != null) {
      return Future<SearchResults>.delayed(
        const Duration(milliseconds: 50),
        () => throw searchError!,
      );
    }
    final r = searchResults;
    return r != null ? Future.value(r) : Completer<SearchResults>().future;
  }

  // ─── Playlists ───────────────────────────────────────────────────────────

  @override
  Future<List<Playlist>> listPlaylists() {
    final p = playlists;
    return p != null ? Future.value(p) : Completer<List<Playlist>>().future;
  }

  @override
  Future<Playlist> getPlaylist(String id) {
    final p = playlist;
    return p != null ? Future.value(p) : Completer<Playlist>().future;
  }

  // ─── URL builders (no network) ───────────────────────────────────────────

  @override
  String streamUrl(String mediaId) =>
      'http://localhost:8080/api/v1/library/songs/$mediaId/stream';

  @override
  String artUrl(String albumId, {int size = 512}) =>
      'http://localhost:8080/api/v1/library/albums/$albumId/art?size=$size';

  @override
  Map<String, String> get authHeaders =>
      const {'Authorization': 'Bearer fake-token'};
}

// ─── FakeAudioHandler ────────────────────────────────────────────────────────
//
// Uses Fake (mockito) — AudioPlayer / just_audio are never initialised.
// Unimplemented methods throw FakeUnimplementedError (a deliberate safety net).
//
// currentMediaItemProvider and playbackStateProvider are overridden at the
// ProviderScope level in the harness, so those streams are never accessed here.
//
// [queue]: items returned by upcomingQueue for QueuePanel goldens.
//   Pass an empty list for the "nothing up next" empty-state golden.

class FakeAudioHandler extends Fake implements SunflowerAudioHandler {
  FakeAudioHandler({
    List<({int queueIndex, MediaItem item})>? queue,
  }) : _queue = queue ?? const [];

  final List<({int queueIndex, MediaItem item})> _queue;

  @override
  List<({int queueIndex, MediaItem item})> get upcomingQueue => _queue;

  @override
  Future<void> play() async {}

  @override
  Future<void> pause() async {}

  @override
  Future<void> skipToNext() async {}

  @override
  Future<void> skipToPrevious() async {}

  @override
  Future<void> seek(Duration position) async {}

  @override
  Future<void> skipToQueueItem(int index) async {}
}

// ─── FakeBufferedApi ─────────────────────────────────────────────────────────
//
// All mutation methods are no-ops.
// [drops] drives the overflow-drop counter shown by SyncStatusWidget.

class FakeBufferedApi extends Fake implements BufferedApiClient {
  FakeBufferedApi({this.drops = 0});

  final int drops;

  @override
  int get overflowDrops => drops;

  @override
  Stream<int> watchPendingCount() => Stream.value(0);

  @override
  Future<void> retryNow() async {}

  @override
  Future<void> like(String mediaId, {required bool liked}) async {}

  @override
  Future<void> scrobble({
    required String mediaId,
    required String queueId,
    required int totalPlayedMs,
    required int durationMs,
    required DateTime occurredAt,
  }) async {}

  @override
  Future<void> addPlaylistItem(String playlistId, String mediaId) async {}

  @override
  Future<void> removePlaylistItem(String playlistId, String mediaId) async {}

  @override
  Future<void> renamePlaylist(String playlistId, String title) async {}

  @override
  Future<void> deletePlaylist(String playlistId) async {}
}
