import 'dart:async';
import 'dart:isolate';

import 'download_worker.dart';

/// Spawns and owns the download worker isolate, exposing a typed two-way
/// channel: send [DownloadRequest]s in, listen to [DownloadEvent]s out.
///
/// One isolate processes requests sequentially (the manager enqueues per-track
/// jobs one-by-one, matching the M6 "jobs enqueued one-by-one" criterion).
class IsolateRunner {
  Isolate? _isolate;
  SendPort? _toWorker;
  final _events = StreamController<DownloadEvent>.broadcast();
  final _ready = Completer<void>();

  /// Stream of progress / completion / failure events from the worker.
  Stream<DownloadEvent> get events => _events.stream;

  /// Spawns the isolate and waits until it is ready to accept requests.
  Future<void> start() async {
    if (_isolate != null) return;
    final fromWorker = ReceivePort();
    _isolate = await Isolate.spawn(downloadIsolateEntry, fromWorker.sendPort);

    fromWorker.listen((msg) {
      if (msg is SendPort) {
        _toWorker = msg;
        if (!_ready.isCompleted) _ready.complete();
      } else if (msg is DownloadEvent) {
        _events.add(msg);
      }
    });
    await _ready.future;
  }

  /// Sends a download request to the worker. [start] must have completed.
  void enqueue(DownloadRequest req) {
    final port = _toWorker;
    if (port == null) {
      throw StateError('IsolateRunner.enqueue before start() completed');
    }
    port.send(req);
  }

  /// Tears down the isolate.
  Future<void> dispose() async {
    _isolate?.kill(priority: Isolate.immediate);
    _isolate = null;
    _toWorker = null;
    await _events.close();
  }
}
