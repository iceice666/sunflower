import 'dart:io';
import 'dart:isolate';

import 'package:dio/dio.dart';

/// Message sent into the worker isolate to start (or resume) a download.
class DownloadRequest {
  const DownloadRequest({
    required this.mediaId,
    required this.url,
    required this.partialPath,
    required this.finalPath,
    this.headers = const {},
  });

  final String mediaId;
  final String url;
  final String partialPath;
  final String finalPath;
  final Map<String, String> headers;
}

/// Progress / lifecycle messages sent back from the worker isolate.
sealed class DownloadEvent {
  const DownloadEvent(this.mediaId);
  final String mediaId;
}

class DownloadProgress extends DownloadEvent {
  const DownloadProgress(super.mediaId, this.received, this.total);
  final int received;
  final int total;
}

class DownloadComplete extends DownloadEvent {
  const DownloadComplete(super.mediaId, this.path, this.bytes);
  final String path;
  final int bytes;
}

class DownloadFailed extends DownloadEvent {
  const DownloadFailed(super.mediaId, this.error);
  final String error;
}

/// Entry point for the download isolate. Receives a [SendPort] for events, then
/// a stream of [DownloadRequest]s on its own receive port. Each request streams
/// bytes to a `.part` file using an HTTP Range header to resume from whatever is
/// already on disk, then atomically renames to the final path on success.
///
/// This runs in a background isolate (see [IsolateRunner]); it must not touch
/// Flutter plugins or the Drift database — it communicates only via messages.
Future<void> downloadIsolateEntry(SendPort toMain) async {
  final commands = ReceivePort();
  toMain.send(commands.sendPort);

  final dio = Dio();

  await for (final msg in commands) {
    if (msg is! DownloadRequest) continue;
    final req = msg;
    try {
      await _runOne(dio, req, toMain);
    } catch (e) {
      toMain.send(DownloadFailed(req.mediaId, e.toString()));
    }
  }
}

Future<void> _runOne(Dio dio, DownloadRequest req, SendPort toMain) async {
  final partial = File(req.partialPath);
  var existing = 0;
  if (await partial.exists()) {
    existing = await partial.length();
  }

  final headers = Map<String, String>.from(req.headers);
  if (existing > 0) {
    headers['Range'] = 'bytes=$existing-';
  }

  final response = await dio.get<ResponseBody>(
    req.url,
    options: Options(
      responseType: ResponseType.stream,
      headers: headers,
      // Accept 200 (full) and 206 (partial/resume).
      validateStatus: (s) => s != null && (s == 200 || s == 206),
    ),
  );

  // If the server ignored the Range and returned 200, restart from scratch.
  final resumed = response.statusCode == 206;
  final sink = partial.openWrite(
    mode: resumed ? FileMode.append : FileMode.write,
  );
  var received = resumed ? existing : 0;

  // Total = received-so-far + the content-length of this response body.
  final contentLen =
      int.tryParse(response.headers.value(Headers.contentLengthHeader) ?? '') ??
          0;
  final total = received + contentLen;

  try {
    await for (final chunk in response.data!.stream) {
      sink.add(chunk);
      received += chunk.length;
      toMain.send(DownloadProgress(req.mediaId, received, total));
    }
    await sink.flush();
    await sink.close();
  } catch (e) {
    await sink.close();
    rethrow; // disk-full / network error → reported as DownloadFailed by caller
  }

  // Atomic publish: rename .part → final.
  await partial.rename(req.finalPath);
  final finalBytes = await File(req.finalPath).length();
  toMain.send(DownloadComplete(req.mediaId, req.finalPath, finalBytes));
}
