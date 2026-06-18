/// Exponential backoff schedule for the write-replay buffer (M7):
/// 5 s → 30 s → 5 min → 30 min → 2 h cap.
///
/// The delay for attempt N (0-indexed: the delay applied AFTER the Nth failed
/// attempt) walks the fixed schedule and saturates at the 2 h cap.
class RetryPolicy {
  const RetryPolicy();

  /// The backoff schedule in milliseconds. The last entry is the cap; attempts
  /// beyond the schedule length reuse it.
  static const schedule = <int>[
    5 * 1000, // 5 s
    30 * 1000, // 30 s
    5 * 60 * 1000, // 5 min
    30 * 60 * 1000, // 30 min
    2 * 60 * 60 * 1000, // 2 h (cap)
  ];

  /// Returns the delay (ms) to wait after [attempts] failed attempts. attempts=1
  /// → 5 s; attempts=2 → 30 s; … attempts>=5 → 2 h.
  int delayMs(int attempts) {
    if (attempts <= 0) return 0;
    final idx = attempts - 1;
    if (idx >= schedule.length) return schedule.last;
    return schedule[idx];
  }

  /// The absolute next-attempt timestamp (ms since epoch) given the current time
  /// and the number of attempts so far.
  int nextAttemptAt(int nowMs, int attempts) => nowMs + delayMs(attempts);
}
