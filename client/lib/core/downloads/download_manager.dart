import 'dart:async';

import 'package:drift/drift.dart' show Value;

import '../api/api_client.dart';
import '../api/sunflower_api.dart';
import '../db/database.dart';
import 'download_worker.dart';
import 'isolate_runner.dart';
import 'storage.dart';
import 'verifier.dart';

/// Public download API: enqueue single tracks or whole playlists, cancel, and
/// observe job status. Owns the worker isolate and reconciles its events back
/// into Drift ([DownloadJobs]/[DownloadedTracks]) and the server registry.
///
/// Job lifecycle: enqueue → pending row → worker streams progress → on complete,
/// verify (local songs) → write DownloadedTracks → register with server. A
/// canceled job stops the worker (best effort) and removes pending state.
class DownloadManager {
  DownloadManager({
    required SunflowerApi api,
    required SunflowerDatabase db,
    required String deviceId,
    BufferedApiClient? bufferedApi,
    IsolateRunner? runner,
    DownloadStorage? storage,
    Verifier? verifier,
  })  : _api = api,
        _db = db,
        _deviceId = deviceId,
        _bufferedApi = bufferedApi,
        _runner = runner ?? IsolateRunner(),
        _storage = storage ?? DownloadStorage(),
        _verifier = verifier ?? Verifier();

  final SunflowerApi _api;
  final SunflowerDatabase _db;
  final String _deviceId;
  final BufferedApiClient? _bufferedApi;
  final IsolateRunner _runner;
  final DownloadStorage _storage;
  final Verifier _verifier;

  final _canceled = <String>{};
  StreamSubscription<DownloadEvent>? _sub;
  bool _started = false;

  /// Starts the worker isolate and resumes any unfinished jobs from a prior run.
  Future<void> start() async {
    if (_started) return;
    _started = true;
    await _runner.start();
    _sub = _runner.events.listen(_onEvent);
    // Resume persisted jobs (Range-resumable from the partial file on disk).
    for (final job in await _db.pendingJobs()) {
      await _dispatch(job.mediaId, job.title, job.sourceUrl, job.playlistId);
    }
  }

  /// Stream of all jobs for the UI.
  Stream<List<DownloadJob>> watchJobs() => _db.watchJobs();

  /// Enqueues a single track download. [streamUrl] is the source to fetch
  /// (server stream URL for local songs; resolved URL for YT, best-effort).
  Future<void> enqueueTrack({
    required String mediaId,
    required String title,
    required String streamUrl,
    String? playlistId,
  }) async {
    _canceled.remove(mediaId);
    await _db.upsertJob(
      DownloadJobsCompanion.insert(
        mediaId: mediaId,
        title: Value(title),
        sourceUrl: streamUrl,
        status: const Value('pending'),
        playlistId: Value(playlistId),
      ),
    );
    await _dispatch(mediaId, title, streamUrl, playlistId);
  }

  /// Enqueues every track of a playlist one-by-one (M6: jobs enqueued
  /// sequentially; a single worker isolate processes them in order).
  Future<void> enqueuePlaylist(String playlistId) async {
    final pl = await _api.getPlaylist(playlistId);
    for (final item in pl.items) {
      final streamUrl = item.mediaId.startsWith('yt:')
          ? (await _api.resolveStream(item.mediaId)).streamUrl
          : _api.streamUrl(item.mediaId);
      await enqueueTrack(
        mediaId: item.mediaId,
        title: item.title,
        streamUrl: streamUrl,
        playlistId: playlistId,
      );
    }
  }

  /// Cancels a pending/in-flight job: stops further processing, drops the job
  /// row, and removes any partial file. (The worker finishes its current chunk;
  /// the completion handler skips a canceled job.)
  Future<void> cancel(String mediaId) async {
    _canceled.add(mediaId);
    await _db.deleteJob(mediaId);
    await _storage.deletePartial(mediaId);
  }

  /// Removes a completed download: deletes the file, the local record, and the
  /// server registry entry.
  Future<void> remove(String mediaId) async {
    await _storage.delete(mediaId);
    await _db.removeDownloadedTrack(mediaId);
    final bufferedApi = _bufferedApi;
    if (bufferedApi != null) {
      await bufferedApi.removeDownload(_deviceId, mediaId);
      return;
    }
    await _api.deleteDownload(_deviceId, mediaId);
  }

  Future<void> _dispatch(
    String mediaId,
    String title,
    String url,
    String? playlistId,
  ) async {
    final partial = await _storage.partialPathFor(mediaId);
    final finalPath = await _storage.pathFor(mediaId);
    await _db.updateJobProgress(mediaId,
        received: 0, total: 0, status: 'running');
    _runner.enqueue(DownloadRequest(
      mediaId: mediaId,
      url: url,
      partialPath: partial,
      finalPath: finalPath,
      headers: _api.authHeaders,
    ));
  }

  Future<void> _onEvent(DownloadEvent e) async {
    if (_canceled.contains(e.mediaId)) return;
    switch (e) {
      case DownloadProgress(:final mediaId, :final received, :final total):
        await _db.updateJobProgress(mediaId, received: received, total: total);
      case DownloadFailed(:final mediaId, :final error):
        await _db.failJob(mediaId, error);
      case DownloadComplete(:final mediaId, :final path, :final bytes):
        await _finish(mediaId, path, bytes);
    }
  }

  Future<void> _finish(String mediaId, String path, int bytes) async {
    // Verify local-library files against the server hash. YouTube downloads are
    // best-effort and accepted without verification (per M6 spec).
    String? sha;
    if (mediaId.startsWith('local:')) {
      try {
        final info = await _api.songHash(mediaId);
        final ok = await _verifier.verify(path, info.sha256);
        if (!ok) {
          await _db.failJob(mediaId, 'sha256 mismatch');
          await _storage.delete(mediaId);
          return;
        }
        sha = info.sha256;
      } catch (_) {
        // Hash endpoint unreachable — keep the file but leave sha null.
      }
    }

    await _db.completeDownload(
      DownloadedTracksCompanion.insert(
        mediaId: mediaId,
        localPath: path,
        bytes: Value(bytes),
        sha256: Value(sha),
      ),
    );

    // Register with the server (M7 reconciles if offline).
    final bufferedApi = _bufferedApi;
    if (bufferedApi != null) {
      await bufferedApi.registerDownload(
        deviceId: _deviceId,
        mediaId: mediaId,
        localPath: path,
        bytes: bytes,
      );
      return;
    }
    await _api.registerDownload(
      deviceId: _deviceId,
      mediaId: mediaId,
      localPath: path,
      bytes: bytes,
    );
  }

  Future<void> dispose() async {
    await _sub?.cancel();
    await _runner.dispose();
  }
}
