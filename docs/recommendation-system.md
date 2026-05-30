# Recommendation System

This document summarizes how Metrolist builds music recommendations and turns that investigation into an implementation blueprint for a similar Android music client.

Research date: 2026-05-15.

## Sources

- Metrolist repo: <https://github.com/MetrolistGroup/Metrolist>
- DeepWiki overview: <https://deepwiki.com/mostafaalagamy/Metrolist>
- DeepWiki YouTube integration: <https://deepwiki.com/mostafaalagamy/Metrolist/3.3-youtube-integration>
- DeepWiki playback notes, including similar content and automix: <https://deepwiki.com/mostafaalagamy/Metrolist/3.1-music-service-and-playback>
- Primary code paths inspected:
  - `app/src/main/kotlin/com/metrolist/music/viewmodels/HomeViewModel.kt`
  - `app/src/main/kotlin/com/metrolist/music/ui/screens/HomeScreen.kt`
  - `app/src/main/kotlin/com/metrolist/music/playback/MusicService.kt`
  - `app/src/main/kotlin/com/metrolist/music/playback/queues/YouTubeQueue.kt`
  - `innertube/src/main/kotlin/com/metrolist/innertube/YouTube.kt`
  - `innertube/src/main/kotlin/com/metrolist/innertube/pages/HomePage.kt`
  - `innertube/src/main/kotlin/com/metrolist/innertube/pages/NextPage.kt`
  - `innertube/src/main/kotlin/com/metrolist/innertube/pages/RelatedPage.kt`

DeepWiki's Metrolist index was last indexed on 2025-08-30, so use it as architectural context only. Current behavior should be verified against the GitHub source.

## What Metrolist Does

Metrolist does not have a single monolithic recommendation engine. It uses a hybrid recommendation surface assembled by `HomeViewModel`:

- Local listening signals from Room: liked songs, recent events, most played songs, most played artists, most played albums, forgotten favorites, related songs, and pinned speed-dial items.
- YouTube Music remote signals through the `innertube` module: home feed, explore feed, `next`, `related`, artist pages, album pages, playlists, account playlists, and continuation pages.
- Playback-side recommendations through `YouTubeQueue.radio(...)`, `startRadioSeamlessly()`, and automix suggestions exposed in the queue UI.
- User preference filters: hide explicit content, hide video songs, hide YouTube Shorts, quick-picks mode, randomize home order, and enable/disable similar content.

DeepWiki describes the same high-level layering: a YouTube integration facade over InnerTube, local persistence through Room/DataStore, and application ViewModels that combine local data, user preferences, and YouTube content before rendering.

## Recommendation Surfaces

### Quick Picks

Metrolist's quick-picks section is local-first:

- In `QuickPicks.QUICK_PICKS` mode it combines `database.quickPicks()`, forgotten favorites, and similar songs derived from the most recent playback event.
- The recent-song branch calls `YouTube.next(WatchEndpoint(videoId = recentSong.id))`, reads `relatedEndpoint`, then calls `YouTube.related(...)`.
- YouTube results are only used if the song already exists locally, keeping this section fast and library-oriented.
- The final set is deduped, shuffled, and capped at 20.

In `QuickPicks.LAST_LISTEN` mode, it takes the latest event song and reads locally stored related songs.

### Daily Discover

Daily Discover is remote-assisted:

- Pick up to 5 shuffled liked-song seeds.
- For each seed, call `YouTube.next(...)` to find a related endpoint.
- Fetch `YouTube.related(...)`.
- Filter explicit content and video songs.
- Pick a recommendation that is not the seed.
- Deduplicate by recommendation ID and shuffle.

This is intentionally lightweight. It avoids training a model and uses YouTube Music's related graph as the candidate generator.

### Similar To Sections

The "similar to" rows are generated from three seed categories:

- Most played artists: fetch `YouTube.artist(seed.id)` and collect items from recent artist sections.
- Most played songs: fetch `YouTube.next(song.id).relatedEndpoint`, then mix songs, albums, artists, and playlists from `YouTube.related(...)`.
- Most played albums: fetch other versions from `YouTube.album(album.id)` and extra artist-section content from the album artist.

The result is stored as `SimilarRecommendation(title, items)` and rendered as horizontal rows on the home screen.

### Community Playlists

Metrolist discovers community-style playlists by:

- Selecting recent most-played artist and song seeds.
- Reading artist pages and related pages.
- Excluding generic YouTube Music, YouTube, "Playlist", RD radio playlists, and OLAK album playlists.
- Fetching each candidate playlist to validate that it contains songs.

This is useful for playlist discovery, but should be rate-limited because it fans out into multiple network calls.

### Home Feed And Explore

`YouTube.home()` returns `HomePage(chips, sections, continuation)`. `HomePage.Section` normalizes multiple YouTube renderer formats into `YTItem` values such as `SongItem`, `AlbumItem`, `ArtistItem`, `PlaylistItem`, `PodcastItem`, and `EpisodeItem`.

`HomeViewModel` filters those sections and exposes continuations through `loadMoreYouTubeItems(...)`. Chip selection calls `YouTube.home(params = chip.endpoint?.params)`.

### Radio And Automix

Playback recommendations sit in the player layer:

- `YouTubeQueue.radio(song)` creates a queue with `WatchEndpoint(videoId = song.id, playlistId = "RDAMVM${song.id}")`.
- `YouTubeQueue.getInitialStatus()` calls `YouTube.next(...)`, handles continuation, and falls back to related-page songs if a radio response is too small.
- `MusicService.startRadioSeamlessly()` replaces the upcoming queue after the current song with radio items.
- `MusicService.getAutomix(...)` fills `automixItems`, which the queue UI displays as "similar content" with "play next" and "add to queue" actions.

## Proposed Architecture

Use the Metrolist pattern, but isolate recommendation logic behind a repository so UI and playback code do not own candidate generation.

```kotlin
enum class RecommendationSource {
    QUICK_PICKS,
    DAILY_DISCOVER,
    SIMILAR_ARTIST,
    SIMILAR_SONG,
    SIMILAR_ALBUM,
    COMMUNITY_PLAYLIST,
    HOME_FEED,
    RADIO,
    AUTOMIX
}

data class RecommendationCandidate(
    val id: String,
    val title: String,
    val type: MediaType,
    val source: RecommendationSource,
    val seedId: String? = null,
    val score: Float = 0f,
    val item: YTItem? = null,
    val localMedia: MediaMetadata? = null,
)

data class RecommendationSection(
    val id: String,
    val title: String,
    val seedId: String? = null,
    val candidates: List<RecommendationCandidate>,
)
```

Recommended components:

- `RecommendationRepository`: candidate generation, filtering, dedupe, cache policy, and scoring.
- `RecommendationViewModel`: state orchestration, loading phases, refresh, chip selection, and UI-facing sections.
- `YouTubeRecommendationDataSource`: wrappers around `home`, `next`, `related`, `artist`, `album`, `playlist`, and continuations.
- `LocalRecommendationDataSource`: wrappers around listening history, liked songs, most played entities, forgotten favorites, pinned items, and related-song maps.
- `RecommendationFilter`: explicit/video/Shorts filters, blocked IDs, duplicate removal, and already-in-section removal.
- `RecommendationCache`: short-lived section cache keyed by source, seed ID, account state, locale, and filters.

## Candidate Pipeline

1. Load fast local sections first.
   - Quick Picks, Keep Listening, Forgotten Favorites, Speed Dial.
   - These should render without waiting for YouTube fan-out.

2. Load remote feed sections in parallel.
   - `home`, `explore`, account playlists, selected chip content.
   - Apply filters before publishing state.

3. Load heavy recommendation sections in background.
   - Daily Discover, Community Playlists, Similar To rows.
   - Publish partial results as each section completes.

4. Normalize and filter candidates.
   - Convert `YTItem` and local `Song` or `Album` objects into one candidate model.
   - Apply explicit, video, Shorts, region, unavailable, and duplicate filters.

5. Rank and diversify.
   - Suggested score:
     - `0.35 * sourceAffinity`
     - `0.20 * seedStrength`
     - `0.15 * recency`
     - `0.15 * novelty`
     - `0.10 * remoteConfidence`
     - `0.05 * diversityBoost`
   - Penalize same artist/album repetition inside a section.
   - Keep deterministic daily shuffling by seeding randomization with local date plus account ID.

6. Persist useful signals.
   - Store played recommendation impressions and clicks.
   - Store successful related-song mappings after playback statistics indicate the user actually listened.
   - Avoid writing every remote candidate to the main song table unless it is played, queued, liked, or saved.

## Section Algorithms

### Quick Picks

Inputs:

- Local related songs.
- Forgotten favorites.
- Recent event's related YouTube songs when available.

Rules:

- Prefer local songs because they are available and already hydrated.
- Do not block the UI on the YouTube branch.
- Cap at 20.
- Refresh when playback history changes or filters change.

### Daily Discover

Inputs:

- Liked songs as seeds.
- `next -> related` graph.

Rules:

- Sample 5-10 liked seeds.
- Fetch related pages concurrently with a small limit.
- Pick 1-3 candidates per seed.
- Deduplicate across seeds.
- Cache for the current date.

### Similar To

Inputs:

- Most played artists, songs, and albums over a recent time window.
- YouTube artist, related, and album pages.

Rules:

- Artists are good broad seeds.
- Songs are good specific seeds.
- Albums should be used sparingly to avoid same-album repetition.
- Render each seed as its own row when there are at least 6 candidates.

### Community Playlists

Inputs:

- Artist pages and song related pages.
- Playlist page validation.

Rules:

- Exclude official auto-generated playlists and album playlists.
- Validate with a playlist fetch before showing.
- Cache for at least 24 hours.
- Limit network fan-out aggressively.

### Radio And Automix

Inputs:

- Current song, current playlist ID, current album ID.
- `YouTube.next(...)`, RDAMVM playlists, related endpoints.

Rules:

- Radio replaces the future queue after the current item.
- Automix is a suggestion shelf, not a committed queue, until the user adds an item or the player reaches the queue edge.
- Respect repeat-all and "similar content" preferences.

## Refresh And Caching Policy

- Local sections: refresh on database event changes, like changes, playlist changes, and content-filter preference changes.
- Home feed: refresh on manual pull-to-refresh, account cookie change, locale change, chip change, and app start after TTL expiry.
- Daily Discover: one daily cache per account/filter/locale tuple.
- Similar To: cache by seed ID and time window.
- Community Playlists: cache by seed ID for 24 hours or longer.
- Radio/Automix: do not persist long-term because stream availability and personalization can change quickly.

## Failure Modes

- YouTube Music may be unavailable in the user's region. Metrolist documents this as a product constraint.
- InnerTube response shapes can change; parsing should be defensive and optional-field tolerant.
- Related endpoints can be null. Always keep a local-only fallback.
- Fan-out sections can overload startup. Split loading into visible local phase and background remote phase.
- Account cookies change personalization. Cache keys must include authenticated/guest state.
- Recommendations can become repetitive. Track recent impressions and add diversity penalties.

## Implementation Steps

1. Add a `RecommendationRepository` and move candidate-generation code out of the home ViewModel.
2. Define candidate and section models that can wrap both local media and YouTube `YTItem` values.
3. Implement local-first Quick Picks and Keep Listening.
4. Add `YouTube.next -> related` seed expansion for Daily Discover and Similar To songs.
5. Add artist and album seed expansion.
6. Add section-level dedupe and ranking.
7. Add short-lived cache and daily cache.
8. Wire UI actions to queues:
   - Song click: `YouTubeQueue.radio(...)` or a direct `YouTubeQueue(...)`.
   - Play all: `ListQueue(...)`.
   - Similar content: add to queue or play next.
9. Add tests with fake local and YouTube data sources:
   - Filters remove explicit/video/Shorts content.
   - Dedupe wins across local and remote duplicates.
   - Daily Discover excludes seed songs.
   - Failed remote calls do not block local sections.
   - Cache keys change when filters or account state change.

## Key Takeaway

Metrolist's recommendation system is practical because it delegates hard personalization to YouTube Music's related graph, then improves product feel with local history, filtering, section composition, and queue integration. Copy that shape: keep startup local-first, use YouTube for candidate generation, centralize filtering and dedupe, and let playback radio/automix handle continuous listening.
