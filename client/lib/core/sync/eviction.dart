/// Buffer-cap eviction policy for the write-replay buffer (M7).
///
/// The buffer holds at most [bufferCap] (10 000) unconfirmed mutations. On
/// overflow the oldest non-confirmed entry is dropped, but priority is honored:
/// likes outrank ordinary mutations, which outrank impression events. The Drift
/// query `evictOldestLowPriority` implements the "lowest priority, then oldest"
/// selection; this file owns the policy constants.
class Eviction {
  /// Maximum number of unconfirmed buffered mutations (M7 locked decision).
  static const bufferCap = 10000;

  /// Priority levels (higher survives eviction longer).
  static const priorityImpression = 0;
  static const priorityDefault = 1;
  static const priorityLike = 2;

  /// Returns the eviction priority for a mutation [kind].
  static int priorityFor(String kind) {
    switch (kind) {
      case 'like':
      case 'unlike':
        return priorityLike;
      case 'impression':
        return priorityImpression;
      default:
        return priorityDefault;
    }
  }

  /// True when the buffer is at or over capacity and must evict before
  /// enqueuing a new entry.
  static bool isOverCap(int currentCount) => currentCount >= bufferCap;
}
