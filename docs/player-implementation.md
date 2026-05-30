# Player Implementation

This document summarizes Metrolist's player architecture and turns it into an implementation blueprint for a Media3/ExoPlayer-based Android music client.

Research date: 2026-05-15.

## Sources

- Metrolist repo: <https://github.com/MetrolistGroup/Metrolist>
- DeepWiki music service and playback: <https://deepwiki.com/mostafaalagamy/Metrolist/3.1-music-service-and-playback>
- DeepWiki YouTube integration: <https://deepwiki.com/mostafaalagamy/Metrolist/3.3-youtube-integration>
- Android Media3 background playback: <https://developer.android.com/media/media3/session/background-playback>
- Android Media3 content library service: <https://developer.android.com/media/media3/session/serve-content>
- Android Media3 ExoPlayer playlists: <https://developer.android.com/media/media3/exoplayer/playlists>
- Android Media3 network stacks: <https://developer.android.com/media/media3/exoplayer/network-stacks>
- Primary code paths inspected:
  - `app/src/main/kotlin/com/metrolist/music/playback/MusicService.kt`
  - `app/src/main/kotlin/com/metrolist/music/playback/PlayerConnection.kt`
  - `app/src/main/kotlin/com/metrolist/music/playback/MediaLibrarySessionCallback.kt`
  - `app/src/main/kotlin/com/metrolist/music/playback/queues/Queue.kt`
  - `app/src/main/kotlin/com/metrolist/music/playback/queues/ListQueue.kt`
  - `app/src/main/kotlin/com/metrolist/music/playback/queues/YouTubeQueue.kt`
  - `app/src/main/kotlin/com/metrolist/music/playback/queues/YouTubePlaylistQueue.kt`
  - `app/src/main/kotlin/com/metrolist/music/ui/player/Player.kt`
  - `app/src/main/kotlin/com/metrolist/music/ui/player/Queue.kt`

DeepWiki's Metrolist index was last indexed on 2025-08-30, so use it as architectural context only. Current behavior should be verified against the GitHub source.

## Target Shape

Metrolist follows the standard Media3 architecture:

```text
Compose UI
  -> PlayerConnection
      -> MusicService
          -> MediaLibrarySession
          -> ExoPlayer
              -> MediaSourceFactory
                  -> ResolvingDataSource
                      -> cache or YouTube stream resolver
```

Android's Media3 docs recommend hosting the player and media session in a service for background playback. Metrolist uses `MediaLibraryService`, which is the right choice when the app also needs a browsable content tree for system clients, Android Auto, search, and media buttons.

## Main Components

### MusicService

`MusicService` is the core owner of playback state. It extends:

- `MediaLibraryService`
- `Player.Listener`
- `PlaybackStatsListener.Callback`

Responsibilities:

- Start and maintain foreground media playback.
- Create and own the primary `ExoPlayer`.
- Create `MediaLibrarySession` for external controllers.
- Resolve YouTube media IDs into expiring stream URLs.
- Manage player and download caches.
- Own the current queue abstraction.
- Persist queue, automix, and player state.
- Handle audio focus, noisy-device events, Bluetooth resume, mute, volume, normalization, equalizer, skip silence, crossfade, sleep timer, and Cast synchronization.
- Classify playback errors and recover or skip/stop.
- Record playback stats to listening history and register playback with YouTube.

### PlayerConnection

`PlayerConnection` is the UI bridge. It is created from the service binder and exposes reactive state:

- `playbackState`
- `isPlaying`
- `mediaMetadata`
- `currentSong`
- `currentLyrics`
- `queueWindows`
- `currentMediaItemIndex`
- `shuffleModeEnabled`
- `repeatMode`
- `canSkipPrevious`
- `canSkipNext`
- `error`
- `waitingForNetworkConnection`

It delegates commands to `MusicService` or the active Cast handler:

- `playQueue`
- `playNext`
- `addToQueue`
- `togglePlayPause`
- `play`
- `pause`
- `seekTo`
- `seekToNext`
- `seekToPrevious`
- `toggleLike`
- `toggleLibrary`
- `toggleMute`

Important Metrolist detail: `PlayerConnection` observes `service.playerFlow` and reattaches itself when the service swaps the active player during crossfade.

### MediaLibrarySessionCallback

`MediaLibrarySessionCallback` exposes the app's library to external clients:

- Root sections: liked songs, songs, artists, albums, playlists, optional YouTube mixes.
- Children: local database songs, artist songs, album songs, playlist songs, downloaded songs, YouTube playlist songs.
- Search: local search plus online YouTube song results.
- `onSetMediaItems`: maps browsed or searched media IDs back into playable media-item lists.
- Custom commands: like, start radio, library toggle, shuffle, repeat, add to target playlist.

This layer keeps Android Auto and system media controls functional without requiring the Compose UI.

### Queue Abstraction

Metrolist defines a small `Queue` interface:

```kotlin
interface Queue {
    val preloadItem: MediaMetadata?
    suspend fun getInitialStatus(): Queue.Status
    fun hasNextPage(): Boolean
    suspend fun nextPage(): List<MediaItem>
}
```

Implementations include:

- `ListQueue`: fixed local list.
- `YouTubeQueue`: dynamic `YouTube.next(...)` queue with radio support.
- `YouTubePlaylistQueue`: YouTube playlist pages plus continuation loading.
- Additional album/radio variants in the playback queue package.

`MusicService.playQueue(...)` sets the active queue, optionally preloads one item, fetches initial status, applies explicit/video filters, sets player media items, restores start index and position, and rebuilds shuffle order when needed.

## Player Creation

Metrolist builds ExoPlayer with:

- `DefaultMediaSourceFactory(createDataSourceFactory(), ExtractorsFactory { MatroskaExtractor(), FragmentedMp4Extractor() })`
- Custom renderers factory for equalizer and silence detection processors.
- `setHandleAudioBecomingNoisy(true)`
- `setWakeMode(C.WAKE_MODE_NETWORK)`
- Media audio attributes: usage media, content type music.
- 5 second seek increments.
- Device volume control.
- `PlaybackStatsListener`.

Recommended baseline:

```kotlin
val player = ExoPlayer.Builder(context)
    .setMediaSourceFactory(mediaSourceFactory)
    .setRenderersFactory(renderersFactory)
    .setHandleAudioBecomingNoisy(true)
    .setWakeMode(C.WAKE_MODE_NETWORK)
    .setAudioAttributes(musicAudioAttributes, false)
    .build()
```

Keep the player in the service, not in the Activity. The Activity should only bind/connect and render state.

## Stream Resolution And Caching

Metrolist does not set permanent YouTube URLs directly on media items. It stores stable media IDs and resolves them at playback time:

1. `ResolvingDataSource.Factory` receives a `DataSpec`.
2. The media ID comes from `dataSpec.key`.
3. Check download cache.
4. Check player cache.
5. Check in-memory `songUrlCache` for an unexpired stream URL.
6. If needed, call `YTPlayerUtils.playerResponseForPlayback(mediaId, audioQuality, connectivityManager)`.
7. Persist `FormatEntity` with itag, mime type, codec, bitrate, sample rate, content length, loudness fields, and playback tracking URL.
8. Store recovered song metadata in the database.
9. Return a stream URL subrange with a fixed chunk length.

This design is important because YouTube stream URLs expire and can be quality-dependent. The stable ID is the media ID; the actual URL is a short-lived transport detail.

## Queue Behavior

Core operations:

- `playQueue(queue)`: replace current queue.
- `playNext(items)`: insert immediately after current item.
- `addToQueue(items)`: append to the end.
- `startRadioSeamlessly()`: preserve the current song and replace only upcoming items with radio recommendations.
- `addToQueueAutomix(item, position)`: remove item from suggestion list and append to queue.
- `playNextAutomix(item, position)`: remove item from suggestion list and insert next.

Useful Metrolist details:

- Duplicate removal is optional through a preference.
- Shuffle is preserved or reset depending on preference.
- `playNext` rebuilds shuffle order so inserted items actually play next.
- `onMediaItemTransition` auto-loads more items when fewer than five remain and the active queue has a next page.
- Repeat-all can disable load-more if configured.
- The queue UI can reorder real queue items through `moveMediaItem` or custom shuffle order.

## Playback State And Events

`MusicService.onMediaItemTransition(...)` handles:

- Episode position save/restore.
- Repeat-one correction if the player auto-advanced.
- Loudness enhancer setup.
- Discord/LastFM/scrobble transitions.
- Cast synchronization.
- Auto-load-more.
- Queue persistence.

`MusicService.onPlaybackStateChanged(...)` handles:

- Ended-state autoplay.
- Repeat-all restart.
- Repeat-one restart.
- Persistent state saving.
- Ready-state retry reset.
- Crossfade scheduling.
- Scrobble stop on idle/ended.

`PlaybackStatsListener` records history after a configurable threshold and registers playback back to YouTube when enough of a track has played.

## Error Recovery

Metrolist's error handling is intentionally defensive:

- Track retry counts per media ID.
- Mark recently failed songs to prevent loops.
- Clear URL cache and player cache on playback errors.
- Force refresh YouTube player/decryption caches.
- Classify:
  - audio renderer errors
  - HTTP 416 range errors
  - page reload errors
  - expired URL or 403 errors
  - missing cache file errors
  - network errors
  - generic IO errors
- Wait for connectivity on network-related failures.
- Use an auto-skip preference to decide between skipping and stopping on final failure.

This is essential for a YouTube-backed client because URLs expire, cache files can become inconsistent, and extractor/audio renderer failures can occur after format changes.

## Audio Features

Metrolist implements several audio features in the service layer:

- Audio focus with pause, resume, and ducking.
- Effective volume as user volume multiplied by sleep timer and audio focus multipliers.
- Mute state.
- Equalizer through a custom audio processor and profile repository.
- Skip silence through Media3's silence skipping plus a custom silence detector for instant skip behavior.
- Audio normalization from YouTube loudness metadata using Android `LoudnessEnhancer`.
- Audio offload preference, disabled when crossfade is enabled.
- Crossfade using a secondary ExoPlayer, then swapping the active player into the media session.

Crossfade is the most delicate part. The UI bridge must observe player swaps, and all listeners, sleep timer state, media session player, audio sessions, and volume ramps must be updated consistently.

## Persistence

Metrolist persists:

- Main queue: `persistent_queue.data`
- Automix queue: `persistent_automix.data`
- Player state: `persistent_player_state.data`

It saves:

- Queue type and title.
- Media item metadata.
- Current media item index.
- Current position.
- Volume, repeat, shuffle, playWhenReady, and playback state.

For a new implementation, prefer a versioned JSON or protobuf schema over Java object serialization. It will be easier to migrate and safer across app versions.

## UI Integration

The Compose player reads `PlayerConnection` state:

- `Player.kt`: now-playing metadata, play/pause state, repeat, skip availability, automix, Cast state, slider position, duration, lyrics/fullscreen modes, player background, and button style.
- `Queue.kt`: queue title, `Timeline.Window` list, current window index, reorder state, swipe removal, selected items, and automix suggestions.

The UI should never resolve streams or own ExoPlayer. It should call `PlayerConnection` methods and observe flows.

## Implementation Plan

1. Service skeleton
   - Create `MusicService : MediaLibraryService`.
   - Start a media playback foreground service.
   - Build `ExoPlayer` and `MediaLibrarySession` in `onCreate`.
   - Release session, players, receivers, and jobs in `onDestroy`.

2. Player bridge
   - Create `PlayerConnection` from a service binder.
   - Expose StateFlows for playback, metadata, queue, shuffle, repeat, errors, and network wait state.
   - Delegate commands to service.

3. Queue layer
   - Add `Queue`, `ListQueue`, `YouTubeQueue`, and `YouTubePlaylistQueue`.
   - Implement initial status and continuation loading.
   - Apply content filters before items reach ExoPlayer.

4. Stream resolver
   - Store media items by stable media ID.
   - Resolve stream URLs at playback time.
   - Add cache-first behavior and expiring URL cache.
   - Persist format/loudness metadata.

5. Playback controls
   - Implement play queue, play next, add to queue, seek next/previous, shuffle, repeat, like, library toggle, radio, and automix.
   - Make Cast routing optional and isolated.

6. State persistence
   - Save queue and player state on transitions and periodically during playback.
   - Restore queue after service startup.
   - Save episode positions separately.

7. Recovery and observability
   - Add retry limits and error classification.
   - Clear corrupt cache resources.
   - Add structured logging for media ID, error code, retry count, quality, and cache path.

8. Advanced audio
   - Add audio focus and volume multipliers early.
   - Add equalizer, normalization, skip silence, and crossfade after baseline playback is stable.

9. External clients
   - Implement `MediaLibrarySessionCallback`.
   - Expose browsable root, children, search, and `onSetMediaItems`.
   - Add custom media session commands for like, radio, library, shuffle, repeat.

## Test Plan

Unit tests:

- Queue initial status and continuation behavior.
- Filter behavior for explicit/video/Shorts content.
- Shuffle-order rebuilding after play-next insertion.
- Retry counter and final-failure decisions.
- Persistence encode/decode and migration.

Integration tests:

- Start service and play a local fake media item.
- Resolve a fake stream URL through the data source.
- Simulate expired URL and verify fresh resolve.
- Simulate network loss and recovery.
- Restore queue after service recreation.
- Verify `PlayerConnection` reattaches after player swap.

Manual QA:

- Background playback and lock-screen controls.
- Android Auto/media browser tree.
- Bluetooth media buttons and noisy-device pause.
- Long queue load-more.
- Radio start from current song.
- Automix add/play-next.
- Crossfade at normal speed, shuffle, repeat-one, repeat-all.
- Offline cached playback.

## Risks

- YouTube stream URLs are expiring implementation details; cache keys must be stable media IDs.
- InnerTube responses can change shape; parsers must tolerate missing fields.
- Service startup races are common; `playQueue` should wait for player initialization.
- Crossfade player swaps can break UI observers if there is no player-flow reattachment.
- Java object serialization can become migration debt.
- Aggressive cache clearing can delete valid downloads if download and player caches are not separated carefully.

## Key Takeaway

Metrolist's player works because playback ownership is centralized in a `MediaLibraryService`, while UI code talks through a narrow reactive bridge. Keep that boundary. Put stream resolution, queue mutation, persistence, audio processing, and recovery in the service; keep Compose focused on state rendering and user commands.
