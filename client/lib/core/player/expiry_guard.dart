import '../api/sunflower_api.dart';

/// Lead time before a stream's `expires_at` at which the guard proactively
/// re-resolves. A 30 s margin covers buffering + the resolve round-trip so the
/// swap lands before playback hits the dead URL.
const kExpiryLeadTime = Duration(seconds: 30);

/// ExpiryGuard decides when a stream URL must be refreshed and performs the
/// refresh via `POST /api/v1/streams/resolve`. It is transport/logic only — the
/// audio handler owns the actual AudioSource swap and position restore.
///
/// Two triggers, matching the M4 acceptance criteria:
///   - near-expiry: `expires_at` is within [kExpiryLeadTime] of now.
///   - hard 403:    just_audio surfaces a PlayerException(403); the handler
///                  calls [resolve] with `proxy: true` to force the fallback.
class ExpiryGuard {
  ExpiryGuard({required SunflowerApi api, DateTime Function()? now})
      : _api = api,
        _now = now ?? DateTime.now;

  final SunflowerApi _api;
  final DateTime Function() _now;

  /// True when [expiresAt] is null-safe expired or within the lead-time window.
  /// Local sources (null expiry) never expire.
  bool needsRefresh(DateTime? expiresAt) {
    if (expiresAt == null) return false;
    return _now().toUtc().add(kExpiryLeadTime).isAfter(expiresAt.toUtc());
  }

  /// Re-resolves [mediaId]. Pass [proxy] true on a hard 403 to force the server
  /// proxy path; near-expiry refreshes use the default direct path.
  Future<ResolvedStream> resolve(String mediaId, {bool proxy = false}) {
    return _api.resolveStream(
      mediaId,
      proxy: proxy,
      reason: proxy ? 'http_403' : 'near_expiry',
    );
  }
}
