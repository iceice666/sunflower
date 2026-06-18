import 'dart:async';

import 'package:audio_service/audio_service.dart';
import 'package:drift/drift.dart' show Value;
import 'package:just_audio/just_audio.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../api/sunflower_api.dart';
import '../db/database.dart';
import 'expiry_guard.dart';
import 'lookahead_loader.dart';
import 'source_resolver.dart';

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
    // just_audio surfaces stream errors (e.g. a 403 on an expired googlevideo
    // URL) on the playback event stream's error channel. In queue mode we
    // recover by forcing a proxy re-resolve of the current source.
    _player.playbackEventStream.listen(
      (_) {},
      onError: (Object e, StackTrace _) {
        if (_queueMode) unawaited(recoverFrom403());
      },
    );
  }

  final _player = AudioPlayer();
  late ConcatenatingAudioSource _playlist;
  List<Song> _songs = [];
  Map<String, String> _authHeaders = {};
  String Function(String mediaId)? _streamUrlBuilder;

  // --- M4 queue mode --------------------------------------------------------
  // Set when playback is driven by a server queue (YouTube radio / mixed
  // catalog) rather than the M2 local-library list. When [_queueMode] is true
  // the handler maintains a lookahead buffer, re-resolves expiring URLs, and
  // falls back to local radio on server loss.
  bool _queueMode = false;
  LookaheadLoader? _loader;
  ExpiryGuard? _expiryGuard;
  LocalRadio? _localRadio;
  SunflowerDatabase? _db;
  SourceResolver? _sourceResolver;
  // Per-source expiry, indexed in lockstep with the ConcatenatingAudioSource
  // children, so the expiry guard knows which entries need a refresh.
  final List<DateTime?> _expiries = [];
  bool _localRadioEngaged = false;

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

  /// Starts M4 queue-mode playback against a server queue. Builds the initial
  /// ConcatenatingAudioSource from the resolved current track and seeds the
  /// lookahead buffer. Wires expiry refresh and local-radio fallback.
  ///
  /// [api] resolves streams and pages `/next`; [db] backs the lookahead cache
  /// and local-radio history.
  Future<void> startQueue({
    required SunflowerApi api,
    required SunflowerDatabase db,
    required String queueId,
    required Map<String, String> authHeaders,
    int position = 0,
  }) async {
    _queueMode = true;
    _authHeaders = authHeaders;
    _db = db;
    _localRadioEngaged = false;
    _loader = LookaheadLoader(api: api, db: db, queueId: queueId);
    _expiryGuard = ExpiryGuard(api: api);
    _localRadio = LocalRadio(db);
    _sourceResolver = SourceResolver(db);
    _expiries.clear();

    final current = await _loader!.start(position);
    if (current == null) {
      // Queue empty/unreachable on cold start → try local radio immediately.
      await _engageLocalRadio();
      return;
    }

    _playlist = ConcatenatingAudioSource(children: []);
    await _appendResolved(current);
    await _player.setAudioSource(_playlist, initialIndex: 0);
    mediaItem.add(_mediaItemForStream(current));
    await _fillBuffer();
    await play();
  }

  /// Upcoming queue entries (after the current track) projected for the UI:
  /// each carries its index in the player sequence so the panel can
  /// `skipToQueueItem`. Empty outside queue mode.
  List<({int queueIndex, MediaItem item})> get upcomingQueue {
    if (!_queueMode) return const [];
    final seq = _player.sequence;
    if (seq == null) return const [];
    final start = (_player.currentIndex ?? 0) + 1;
    final out = <({int queueIndex, MediaItem item})>[];
    for (var i = start; i < seq.length; i++) {
      final tag = seq[i].tag;
      if (tag is MediaItem) out.add((queueIndex: i, item: tag));
    }
    return out;
  }

  /// Appends a resolved stream as an AudioSource and records its expiry in
  /// lockstep so [_refreshIfNeeded] can target it later. Also records the play
  /// into local history for the offline radio fallback.
  Future<void> _appendResolved(ResolvedStream s) async {
    // Prefer a downloaded local file over the network URL (M6): if this media
    // is downloaded, play from disk and never touch the network.
    final localUri = await _sourceResolver?.localUriFor(s.mediaId);
    final uri = localUri ?? s.streamUrl;
    final isLocalFile = localUri != null;
    await _playlist.add(
      AudioSource.uri(
        Uri.parse(uri),
        // file:// and local-server URLs both authenticate differently: a
        // downloaded file needs no headers; a local-server stream does.
        headers: (isLocalFile || s.source != 'local') ? null : _authHeaders,
        tag: _mediaItemForStream(s),
      ),
    );
    // A local file never expires; otherwise track the network URL's expiry.
    _expiries.add(isLocalFile ? null : s.expiresAt);
    unawaited(_recordPlay(s));
  }

  /// Resolves and appends buffered items until the player has ≥kMinBuffer
  /// upcoming sources. Network failures stop the fill silently — the buffered
  /// items already queued still play, and local radio covers exhaustion.
  Future<void> _fillBuffer() async {
    final loader = _loader;
    final guard = _expiryGuard;
    if (loader == null || guard == null) return;
    final currentIdx = _player.currentIndex ?? 0;
    while ((_playlist.length - currentIdx - 1) < kMinBuffer) {
      if (loader.bufferLength == 0) {
        try {
          await loader.ensureBuffer();
        } catch (_) {
          return; // server unreachable; rely on what is buffered
        }
      }
      final item = loader.takeNext();
      if (item == null) break; // queue exhausted
      try {
        final resolved = await guard.resolve(item.mediaId);
        await _appendResolved(resolved);
      } catch (_) {
        break; // resolve failed; stop filling, fall through to fallback later
      }
    }
  }

  /// Re-resolves the source at [index] if its URL is expired/near-expiry, then
  /// swaps it in place and restores position. Returns true if a swap happened.
  Future<bool> _refreshIfNeeded(int index, {bool force = false}) async {
    final guard = _expiryGuard;
    if (guard == null || index < 0 || index >= _expiries.length) return false;
    if (!force && !guard.needsRefresh(_expiries[index])) return false;

    final tag = _player.sequence?[index].tag as MediaItem?;
    final mediaId = tag?.id;
    if (mediaId == null) return false;

    final pos = _player.position;
    final resolved = await guard.resolve(mediaId, proxy: force);
    await _playlist.removeAt(index);
    await _playlist.insert(
      index,
      AudioSource.uri(
        Uri.parse(resolved.streamUrl),
        headers: resolved.source == 'local' ? _authHeaders : null,
        tag: _mediaItemForStream(resolved),
      ),
    );
    _expiries[index] = resolved.expiresAt;
    if (_player.currentIndex == index) {
      await _player.seek(pos, index: index);
    }
    return true;
  }

  /// Engages the offline local-radio fallback: appends recent-play items as
  /// local AudioSources. Called when the server is unreachable and the buffer
  /// is exhausted.
  Future<void> _engageLocalRadio() async {
    if (_localRadioEngaged) return;
    final radio = _localRadio;
    if (radio == null) return;
    final items = await radio.fromRecentPlays();
    if (items.isEmpty) return;
    _localRadioEngaged = true;
    for (final entry in items) {
      await _playlist.add(
        AudioSource.uri(
          Uri.parse(entry.url),
          headers: _authHeaders,
          tag: _mediaItemForQueueItem(entry.item),
        ),
      );
      _expiries.add(null); // local sources never expire
    }
  }

  Future<void> _recordPlay(ResolvedStream s) async {
    final db = _db;
    if (db == null) return;
    try {
      await db.recordPlay(
        RecentPlaysCompanion.insert(
          mediaId: s.mediaId,
          title: Value(s.title),
          artistName: Value(s.artists.isEmpty ? '' : s.artists.first),
          source: Value(s.source),
          // Only cache replayable (local) URLs; YT URLs expire and are useless
          // offline.
          streamUrl: s.source == 'local'
              ? Value(s.streamUrl)
              : const Value(null),
          durationMs: Value(s.durationMs),
        ),
      );
    } catch (_) {
      // History is advisory.
    }
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
    if (index == null) return;
    if (_queueMode) {
      _onQueueIndexChanged(index);
      return;
    }
    if (index >= _songs.length) return;
    mediaItem.add(_mediaItemFor(_songs[index]));
    _persistLastPlayed();
  }

  /// Queue-mode track transition: publish metadata from the source tag, top up
  /// the lookahead buffer, refresh the now-current source if it is expiring,
  /// and engage local radio when the buffer is exhausted and the server is
  /// unreachable.
  void _onQueueIndexChanged(int index) {
    final tag = _player.sequence?[index].tag as MediaItem?;
    if (tag != null) mediaItem.add(tag);
    unawaited(_onQueueAdvance(index));
  }

  Future<void> _onQueueAdvance(int index) async {
    // Refresh the current source if its URL is at/near expiry before it plays.
    try {
      await _refreshIfNeeded(index);
    } catch (_) {
      // fall through; a hard 403 during playback is handled by recoverFrom403
    }
    // Keep the buffer full.
    final remaining = _playlist.length - index - 1;
    if (remaining < kMinBuffer) {
      await _fillBuffer();
    }
    // Buffer exhausted with no more server items → offline fallback.
    if (_playlist.length - index - 1 == 0 &&
        (_loader == null || !_loader!.hasMore)) {
      await _engageLocalRadio();
    }
  }

  /// Hard-403 recovery entry point for the player error listener: forces a
  /// proxy re-resolve of the current source and resumes from position.
  /// Returns true if recovery swapped a source.
  Future<bool> recoverFrom403() async {
    final idx = _player.currentIndex;
    if (!_queueMode || idx == null) return false;
    try {
      return await _refreshIfNeeded(idx, force: true);
    } catch (_) {
      await _engageLocalRadio();
      return false;
    }
  }

  Future<void> _persistLastPlayed() async {
    final idx = _player.currentIndex;
    if (idx == null || idx >= _songs.length) return;
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString(_kLastMediaId, _songs[idx].mediaId);
    await prefs.setInt(_kLastPosition, _player.position.inMilliseconds);
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

  static MediaItem _mediaItemForStream(ResolvedStream s) {
    return MediaItem(
      id: s.mediaId,
      title: s.title.isEmpty ? s.mediaId : s.title,
      artist: s.artists.isEmpty ? null : s.artists.join(', '),
      duration: s.durationMs > 0 ? Duration(milliseconds: s.durationMs) : null,
    );
  }

  static MediaItem _mediaItemForQueueItem(QueueItem it) {
    return MediaItem(
      id: it.mediaId,
      title: it.title.isEmpty ? it.mediaId : it.title,
      artist: it.artists.isEmpty ? null : it.artists.join(', '),
      duration: it.durationMs > 0
          ? Duration(milliseconds: it.durationMs)
          : null,
    );
  }
}
