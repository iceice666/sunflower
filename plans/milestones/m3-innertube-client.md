# M3 — InnerTube Client (Archived Go Plan)

> **Archive note (2026-07-01):** This milestone is retained as historical
> build and acceptance context from the original Go `server/` implementation.
> The canonical implementation is now Rust under `rust/`; use
> [`../README.md`](../README.md) and [`../architecture.md`](../architecture.md)
> for current crate layout, migrations, assets, and verification commands.

## Demo target

```
$ probe innertube next --video-id=dQw4w9WgXcQ
{
  "current_url":   "https://rr3---sn-xxx.googlevideo.com/videoplayback?…",
  "expires_at":    "2026-05-30T16:00:00Z",
  "itag":          251,
  "next_items":    [{"video_id":"…","title":"…"}, …],
  "continuation":  "…"
}

$ curl -o /tmp/song.webm "$(probe innertube next --video-id=dQw4w9WgXcQ -o url)"
$ ffprobe /tmp/song.webm   # plays back, sane duration
```

The Go server can resolve YouTube video IDs to fresh playable stream URLs
without any external service.

**This is the single largest milestone** — give it dedicated focus.

## Scope

- HTTP client with cookie injection (`internal/innertube/client.go`).
- Signature decryption (`internal/innertube/sig/`).
- Player-response parsing.
- Renderer normalizers for: `home`, `next`, `related`, `artist`, `album`,
  `playlist`, `search`.
- Continuation cursor handling.
- Optional cookie storage encryption (`internal/cookies`) — needed for
  personalization but a guest-mode probe should also work.
- `cmd/probe` extended with `innertube` subcommands.

## Files to create

```
server/internal/innertube/
  client.go                    # HTTP client, locale config, base context builder
  context.go                   # ANDROID_MUSIC / WEB_REMIX client contexts
  sig/
    base_js.go                 # fetch + cache base.js
    transform.go               # parse + apply sig op list (reverse/splice/swap)
    cipher.go                  # n-decoding (if still needed)
  payloads/
    player.go                  # POST /youtubei/v1/player
    next.go                    # POST /youtubei/v1/next
    browse.go                  # POST /youtubei/v1/browse
    search.go                  # POST /youtubei/v1/search
  parser/
    yt_item.go                 # SongItem, AlbumItem, ArtistItem, PlaylistItem
    home_page.go               # HomePage with sections + chips
    next_page.go               # WatchEndpoint, relatedEndpoint, continuation
    related_page.go
    artist_page.go
    album_page.go
    playlist_page.go
  continuation/
    cursor.go                  # opaque token preservation
  models/
    models.go                  # public Go structs
  testdata/                    # fixture JSON captured from real YT calls
    home_normal.json
    home_missing_chips.json
    next_with_continuation.json
    next_no_related.json
    player_with_sig.json
    …
server/internal/cookies/
  store.go                     # libsodium secretbox encrypt/decrypt
  refresh_job.go               # hourly health-probe with a known video
server/cmd/probe/
  main.go
  innertube_cmd.go             # subcommands: home, next, search, cookies-set
```

## Acceptance criteria

- `probe innertube next --video-id=<any popular video>` returns a fresh URL
  that `curl` can `200 OK` and stream from.
- Sig decryption works against the current base.js and is unit-tested with
  frozen pairs under `internal/innertube/sig/testdata/`.
- All parser tests pass — including missing-field cases (parsers return
  zero-valued structs, never error).
- Continuation tokens round-trip: parse, store, post back, get next page.
- Cookies uploaded via `POST /api/v1/cookies/youtube` are stored encrypted; the
  cookie health probe runs hourly and updates a status field.
- Guest-mode (no cookies) works for `next`/`related`; home feed degrades to
  generic content with a logged warning.

## Dependencies on prior milestones

- M0 server bootstrap.
- M1 auth (for cookie upload endpoint).

## Verification

- **Table-driven parser unit tests** for every renderer with fixture JSON.
  Naming: `TestHomePage_NormalShape`, `TestHomePage_MissingChips`,
  `TestNextPage_NoContinuation`. Each "missing X" case must return a
  zero-value field, never an error.
- **Sig transform tests** with frozen base.js → frozen sig in/out pairs (the
  test must continue to pass after a re-record when YT rotates base.js).
- **End-to-end manual test:** `probe innertube next` for ~20 different videos,
  diversity of formats, regions, and ages; all must produce a working URL.
- **Cookie probe test:** upload cookies, run probe job, verify status row
  updates.

## Risks specific to M3

- See [`../risks.md`](../risks.md) §§1, 2, 3.
- Time estimate is the largest of any milestone. **Don't proceed to M4 until
  M3 acceptance is solid** — every later milestone depends on this.

## Out of M3 scope

- Personalized home/explore parsing beyond what the renderer normalizers
  cover. The *recommendation* surface assembly lives in M5.
- Stream proxy fallback (M4).
