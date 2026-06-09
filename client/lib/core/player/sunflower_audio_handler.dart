import 'package:audio_service/audio_service.dart';
import 'package:just_audio/just_audio.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../api/sunflower_api.dart';

const _kLastMediaId = 'last_media_id';
const _kLastPosition = 'last_position_ms';

/// BaseAudioHandler subclass that drives just_audio and publishes
/// media metadata + playback state to the OS media session.
///
/// M2 prev/next: the full song list is loaded as an ephemeral in-memory
/// ConcatenatingAudioSource so prev/next traverse it.  This is NOT the M4
/// queue/lookahead feature — no server queue, no persistence, no prefetch.
class SunflowerAudioHandler extends BaseAudioHandler {
  SunflowerAudioHandler() {
    _player.playbackEventStream.map(_transformEvent).pipe(playbackState);
    _player.currentIndexStream.listen(_onIndexChanged);
  }

  final _player = AudioPlayer();
  late ConcatenatingAudioSource _playlist;
  List<Song> _songs = [];
  Map<String, String> _authHeaders = {};
  String Function(String mediaId)? _streamUrlBuilder;

  // ---------------------------------------------------------------------------
  // Public API — called by the UI / player_bootstrap providers
  // ---------------------------------------------------------------------------

  /// Loads [songs] as the in-memory playlist and starts playing at [index].
  Future<void> loadPlaylist(
    List<Song> songs,
    int index,
    String Function(String mediaId) streamUrlBuilder,
    Map<String, String> authHeaders,
  ) async {
    _songs = songs;
    _authHeaders = authHeaders;
    _streamUrlBuilder = streamUrlBuilder;

    final sources = songs.map((s) {
      return AudioSource.uri(
        Uri.parse(streamUrlBuilder(s.mediaId)),
        headers: authHeaders,
        tag: _mediaItemFor(s),
      );
    }).toList();

    _playlist = ConcatenatingAudioSource(children: sources);
    await _player.setAudioSource(_playlist, initialIndex: index);
    mediaItem.add(_mediaItemFor(songs[index]));
    await play();
  }

  // ---------------------------------------------------------------------------
  // BaseAudioHandler overrides
  // ---------------------------------------------------------------------------

  @override
  Future<void> play() => _player.play();

  @override
  Future<void> pause() async {
    await _player.pause();
    await _persistLastPlayed();
  }

  @override
  Future<void> stop() async {
    await _player.stop();
    await _persistLastPlayed();
  }

  @override
  Future<void> seek(Duration position) => _player.seek(position);

  @override
  Future<void> skipToNext() => _player.seekToNext();

  @override
  Future<void> skipToPrevious() => _player.seekToPrevious();

  @override
  Future<void> skipToQueueItem(int index) async {
    await _player.seek(Duration.zero, index: index);
    await play();
  }

  @override
  Future<void> onTaskRemoved() async {
    await _persistLastPlayed();
    await stop();
  }

  // ---------------------------------------------------------------------------
  // Restore last played
  // ---------------------------------------------------------------------------

  /// Returns the persisted (mediaId, position) from a previous session, or
  /// null if no prior session exists.
  static Future<(String, Duration)?> loadLastPlayed() async {
    final prefs = await SharedPreferences.getInstance();
    final id = prefs.getString(_kLastMediaId);
    final ms = prefs.getInt(_kLastPosition);
    if (id == null) return null;
    return (id, Duration(milliseconds: ms ?? 0));
  }

  // ---------------------------------------------------------------------------
  // Internals
  // ---------------------------------------------------------------------------

  void _onIndexChanged(int? index) {
    if (index == null || index >= _songs.length) return;
    mediaItem.add(_mediaItemFor(_songs[index]));
    _persistLastPlayed();
  }

  Future<void> _persistLastPlayed() async {
    final idx = _player.currentIndex;
    if (idx == null || idx >= _songs.length) return;
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString(_kLastMediaId, _songs[idx].mediaId);
    await prefs.setInt(
        _kLastPosition, _player.position.inMilliseconds);
  }

  PlaybackState _transformEvent(PlaybackEvent event) {
    final playing = _player.playing;
    return PlaybackState(
      controls: [
        MediaControl.skipToPrevious,
        if (playing) MediaControl.pause else MediaControl.play,
        MediaControl.skipToNext,
      ],
      systemActions: const {
        MediaAction.seek,
        MediaAction.seekForward,
        MediaAction.seekBackward,
      },
      androidCompactActionIndices: const [0, 1, 2],
      processingState: const {
        ProcessingState.idle: AudioProcessingState.idle,
        ProcessingState.loading: AudioProcessingState.loading,
        ProcessingState.buffering: AudioProcessingState.buffering,
        ProcessingState.ready: AudioProcessingState.ready,
        ProcessingState.completed: AudioProcessingState.completed,
      }[_player.processingState]!,
      playing: playing,
      updatePosition: _player.position,
      bufferedPosition: _player.bufferedPosition,
      speed: _player.speed,
      queueIndex: _player.currentIndex,
    );
  }

  static MediaItem _mediaItemFor(Song song) {
    return MediaItem(
      id: song.mediaId,
      title: song.title,
      artist: song.artistName.isEmpty ? null : song.artistName,
      album: song.albumTitle.isEmpty ? null : song.albumTitle,
      duration: song.durationMs != null
          ? Duration(milliseconds: song.durationMs!)
          : null,
      // artUri is omitted here — populated separately by the UI after
      // downloading art to a local cache file (see NowPlayingScreen).
      // The OS lock-screen loader may not send Authorization headers,
      // so a file:// URI is safer than an https:// URI with auth.
    );
  }
}
