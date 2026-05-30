# M5 — Recommendation Pipeline + Home Feed

## Demo target

Open Flutter app. Home tab shows multiple sections:

- Quick Picks (local-first, capped at 20)
- Daily Discover (5 liked seeds → related expansion)
- Similar to <recent artist> (one row per top-3 most-played artists)
- YouTube Music home feed (with chips: Relax, Workout, Sleep, …)
- Community Playlists

Pull to refresh → updates. Kill server → cold-start still shows yesterday's
cached sections from `HomeCache`.

## Scope

- Recommendation engine (`internal/recs`).
- All section builders mirroring Metrolist's `HomeViewModel`.
- Filter pipeline (`notExplicit`, `notVideo`, `notShorts`, `notBlocked`,
  `notRecentImpression`, `notDuplicateInSection`).
- Ranking with the 0.35/0.20/0.15/0.15/0.10/0.05 weights.
- Cache layer keyed by user/source/seed/locale/region/filters_hash.
- `GET /api/v1/home` handler.
- Playlist CRUD (needed for Similar / Community Playlists).
- Like toggle.
- Impression logging (`recommendation_impressions`).
- Flutter home screen with cold-start cache + section rendering.

## Files to create

```
server/internal/recs/
  engine.go                # public API: BuildHome, QuickPicks, DailyDiscover, …
  pipeline.go              # candidate -> filter -> rank -> diversify
  filters.go               # composable predicates
  ranking.go               # the weight formula + sub-scorers
  affinity.go              # sourceAffinity table
  seed_strength.go         # normalized play count per seed
  recency.go               # exponential decay
  novelty.go               # 1 - hits_in_impressions_24h / cap
  diversity.go             # per-section artist/album spread
  cache.go                 # rec_cache read/write with TTL per source
  section_quick_picks.go
  section_daily_discover.go
  section_similar_artist.go
  section_similar_song.go
  section_similar_album.go
  section_community_playlists.go
  section_radio.go         # used by /queue/start artist_radio
server/internal/api/
  handlers_home.go         # GET /home with continuation
  handlers_likes.go        # POST /likes
  handlers_playlists.go    # full CRUD
  handlers_impressions.go  # POST /impressions (batch)
server/db/queries/
  recs.sql                 # MostPlayedSongs, MostPlayedArtists,
                           # ForgottenFavorites, RecentImpressions, …

client/lib/features/home/
  home_screen.dart
  home_controller.dart     # fetch + cache
  section_widget.dart      # horizontal row
  chip_bar.dart
client/lib/core/db/
  database.dart            # add HomeCache, RecentImpressionsCache
client/lib/features/library/
  playlists_screen.dart
  playlist_detail_screen.dart
```

## Acceptance criteria

- `GET /api/v1/home` returns ≥4 sections under normal cookies + library state.
- Quick Picks renders within 500 ms of receiving the response (local-first
  section, no network in critical path).
- Section caches honour their TTL — second `GET /home` within 30 min is served
  from `rec_cache`.
- Daily Discover excludes seed songs themselves (impression dedupe).
- Filters wired to user preferences screen: `hide_explicit`, `hide_video`,
  `hide_shorts` all functional.
- Ranking unit tests pass with synthetic candidates: set 5 weights to 0,
  vary one, verify the ordering changes accordingly.
- Like toggle from any home tile updates `likes` table and refreshes Quick
  Picks/Daily Discover on next fetch.
- Cold-start (server unreachable on app launch) → `HomeCache` renders
  yesterday's content with a "stale" indicator.

## Dependencies on prior milestones

- M3 InnerTube — provides the candidate generator.
- M4 queue/streams — needed for tap-to-play from a section.

## Verification

- Ranking unit tests as above.
- Filter unit tests with synthetic candidates.
- Section algorithm tests for each builder.
- Cache TTL tests: time travel via injected clock; verify hit/miss boundaries.
- Integration test: full `/home` with fake InnerTube returning fixture pages.
- Manual: open home; verify each section type appears; refresh; verify update;
  toggle a hide-explicit filter; verify explicit items disappear on next fetch.

## Out of M5 scope

- Offline downloads of recommended tracks (M6).
- Cross-device impression / like sync semantics (M7).
- Lyrics, Discord scrobble — see [`../risks.md`](../risks.md) out-of-scope.
