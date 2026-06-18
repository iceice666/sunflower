import 'package:uuid/uuid.dart';

/// Generates UUIDv7 idempotency keys. v7 embeds a millisecond timestamp in its
/// high bits, so lexically/numerically increasing keys also order by creation
/// time — the replay buffer relies on this for client-clock ordering.
class IdempotencyKeys {
  IdempotencyKeys([Uuid? uuid]) : _uuid = uuid ?? const Uuid();

  final Uuid _uuid;

  /// Returns a fresh UUIDv7 string.
  String next() => _uuid.v7();
}
