# Risks and Out of Scope

## Top 5 risks

1. **InnerTube parser drift.** YouTube changes renderer shapes without notice.
   Mitigation: defensive parsers (zero-value on missing fields, never error),
   fixture corpus committed under `internal/innertube/parser/testdata/`,
   structured logging of every renderer kind seen vs. expected so anomalies
   surface fast.

2. **Signature decryption breaks.** YT rotates `base.js` semantics
   periodically. Mitigation: cache parsed sig ops by base.js hash; on a
   sustained 403 burst, invalidate the cache and re-parse; alerting on
   sustained 403 rate; manual override env var to pin a known-good base.js URL
   for emergencies.

3. **Cookie poisoning / rotation.** Stored YT cookies expire or get
   region-flagged. Mitigation: cookie health probe every 1 h (cheap `next`
   call against a known video); admin endpoint reports cookie status; graceful
   degradation to guest-mode InnerTube when cookies fail.

4. **Write-replay buffer overflow under prolonged offline.** A user could go
   weeks offline and overflow the 10 000-entry buffer. Mitigation: drop oldest
   *non-confirmed* by insertion order; UI surfaces "N changes pending sync"
   so the user notices before overflow; eviction priority keeps likes above
   recommendation impressions (which are easier to recreate).

5. **Cross-platform `just_audio` quirks** (especially desktop/web). Mitigation:
   ship Android first as the M2 demo target; desktop and web come after M8;
   maintain a platform-capability matrix in `core/player/capabilities.dart`
   and degrade UI features gracefully where platform support is missing.

6. **Open enrollment exposure before M9.** In M0-M8, any network peer that can
   reach the server can call `register-device` and obtain a valid device token.
   Mitigation: M9 replaces open registration with owner setup, admin sessions,
   one-time pairing codes, device revocation, rate limits, and audit events.

## Out of scope for v1 (opinionated)

- Chromecast / AirPlay routing
- Multi-user / household accounts (schema is multi-user-ready but auth and
  recommendation scoping are single-user only)
- SponsorBlock-style segment skipping
- Lyrics (no provider, no UI, no DB schema)
- Discord / LastFM scrobbling
- Equalizer / loudness normalization UI (server can include `loudness_db` for
  later use, but client doesn't apply it)
- Podcasts / episodes (Metrolist supports them; v1 ignores)
- Android Auto / CarPlay surfaces
- Audio offload / silence skipping as user-facing features
- Crossfade is M8 optional, behind a setting
- Server-side admin web UI is out of scope for v1; it is planned as M10
