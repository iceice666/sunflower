import 'dart:async';
import 'dart:convert';

import 'package:dio/dio.dart';
import 'package:drift/drift.dart' show Value;

import '../db/database.dart';
import 'eviction.dart';
import 'idempotency_key.dart';
import 'retry_policy.dart';

/// The write-replay buffer (M7). Every mutating API call is enqueued here first,
/// then drained to the server in client-clock order with exponential backoff.
/// Replays are idempotent (the Idempotency-Key is the row's primary key, which
/// the server dedupes), so re-sending a confirmed mutation never double-applies.
///
/// Concurrency: a single drain runs at a time (guarded by [_draining]); the
/// buffer is otherwise driven by enqueue calls and a periodic timer.
class ReplayBuffer {
  ReplayBuffer({
    required Dio dio,
    required SunflowerDatabase db,
    IdempotencyKeys? keys,
    RetryPolicy retry = const RetryPolicy(),
    int Function()? nowMs,
  })  : _dio = dio,
        _db = db,
        _keys = keys ?? IdempotencyKeys(),
        _retry = retry,
        _now = nowMs ?? (() => DateTime.now().millisecondsSinceEpoch);

  final Dio _dio;
  final SunflowerDatabase _db;
  final IdempotencyKeys _keys;
  final RetryPolicy _retry;
  final int Function() _now;

  bool _draining = false;
  int _clock = 0;
  int _overflowDrops = 0;
  Timer? _timer;

  /// Number of buffered mutations dropped due to overflow (surfaced in the
  /// sync-status UI).
  int get overflowDrops => _overflowDrops;

  /// Live count of unconfirmed mutations (the "N pending" indicator).
  Stream<int> watchPendingCount() => _db.watchPendingCount();

  /// Starts a periodic drain. Safe to call once at app start.
  void start({Duration interval = const Duration(seconds: 5)}) {
    _timer ??= Timer.periodic(interval, (_) => drain());
  }

  void dispose() {
    _timer?.cancel();
    _timer = null;
  }

  /// Enqueues a mutation for replay. Duplicate keys keep the original row;
  /// new rows evict on overflow before inserting so the buffer never exceeds
  /// the cap. Returns the idempotency key used.
  Future<String> enqueue({
    required String kind,
    required String method,
    required String path,
    Map<String, dynamic> body = const {},
    String? idempotencyKey,
  }) async {
    final key = idempotencyKey ?? _keys.next();
    if (await _db.hasMutation(key)) {
      await drain();
      return key;
    }

    // Overflow handling: evict the lowest-priority oldest entry first.
    if (Eviction.isOverCap(await _db.pendingCount())) {
      final evicted = await _db.evictOldestLowPriority();
      if (evicted != null) _overflowDrops++;
    }

    final clock = _nextClock();
    await _db.enqueueMutation(
      PendingMutationsCompanion.insert(
        idempotencyKey: key,
        kind: kind,
        method: method,
        path: path,
        bodyJson: Value(jsonEncode(body)),
        clientClock: clock,
        priority: Value(Eviction.priorityFor(kind)),
      ),
    );
    // Try to drain promptly (online path); failures reschedule with backoff.
    await drain();
    return key;
  }

  /// Drains all due mutations in client-clock order. Transient failures are
  /// rescheduled with backoff and stop this drain to preserve dependency order.
  /// Permanent 4xx failures are discarded so one stale mutation cannot block
  /// newer offline feedback forever.
  Future<void> drain() async {
    if (_draining) return;
    _draining = true;
    try {
      final due = await _db.dueMutations(_now());
      for (final m in due) {
        final ok = await _replayOne(m);
        if (!ok) {
          // Stop the batch on the first failure to preserve strict clock order
          // for dependent mutations; backoff will retry from here next tick.
          break;
        }
      }
      await _db.purgeConfirmed();
    } finally {
      _draining = false;
    }
  }

  Future<bool> _replayOne(PendingMutation m) async {
    late final Map<String, dynamic> body;
    try {
      body = _decodeBody(m.bodyJson);
    } catch (e) {
      await _db.discardMutation(m.idempotencyKey);
      return true;
    }

    try {
      await _dio.request<dynamic>(
        m.path,
        data: m.method == 'DELETE' ? null : body,
        options: Options(
          method: m.method,
          headers: {'Idempotency-Key': m.idempotencyKey},
        ),
      );
      await _db.confirmMutation(m.idempotencyKey);
      return true;
    } catch (e) {
      if (_isPermanentReplayFailure(e)) {
        await _db.discardMutation(m.idempotencyKey);
        return true;
      }
      final attempts = m.attempts + 1;
      await _db.reschedule(
        m.idempotencyKey,
        attempts: attempts,
        nextAttemptAt: _retry.nextAttemptAt(_now(), attempts),
        error: e.toString(),
      );
      return false;
    }
  }

  Map<String, dynamic> _decodeBody(String bodyJson) {
    if (bodyJson.trim().isEmpty) return {};
    final decoded = jsonDecode(bodyJson);
    if (decoded is! Map) {
      throw const FormatException('pending mutation body must be an object');
    }
    return decoded.cast<String, dynamic>();
  }

  bool _isPermanentReplayFailure(Object error) {
    if (error is! DioException) return false;
    final status = error.response?.statusCode;
    if (status == null) return false;
    if (status == 401 || status == 408 || status == 429) return false;
    return status >= 400 && status < 500;
  }

  int _nextClock() {
    final t = _now();
    // Monotonic even if the wall clock doesn't advance between two enqueues.
    _clock = t > _clock ? t : _clock + 1;
    return _clock;
  }
}
