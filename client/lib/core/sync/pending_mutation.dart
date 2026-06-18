/// Status of a buffered mutation as it moves through the replay state machine.
enum MutationStatus { pending, inflight, confirmed, failed }

/// A mutation queued for replay. This is the in-memory shape the buffer and API
/// wrapper pass around; it maps 1:1 to the PendingMutations Drift row.
class PendingMutationData {
  const PendingMutationData({
    required this.idempotencyKey,
    required this.kind,
    required this.method,
    required this.path,
    required this.bodyJson,
    required this.clientClock,
    required this.priority,
  });

  final String idempotencyKey;
  final String kind;
  final String method; // POST | PATCH | DELETE
  final String path;
  final String bodyJson;
  final int clientClock;
  final int priority;
}
