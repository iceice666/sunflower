import 'dart:io';

import 'package:drift/drift.dart';
import 'package:drift/native.dart';
import 'package:path/path.dart' as p;
import 'package:path_provider/path_provider.dart';

part 'database.g.dart';

// ---------------------------------------------------------------------------
// Tables
// ---------------------------------------------------------------------------

/// Cold-start cache of the most recent `/next` window. On a cold launch with the
/// server unreachable we can re-seed the player from the last persisted queue
/// instead of showing an empty screen. Keyed by (queueId, position).
///
/// Rows are written as the lookahead loader buffers each item and pruned when a
/// new queue starts. `streamUrl`/`streamExpiresAt` are best-effort: a cached URL
/// is very likely expired on cold start, so the expiry guard re-resolves before
/// playback. The value of the cache is the *item list*, not the URLs.
class LookaheadCache extends Table {
  TextColumn get queueId => text()();
  IntColumn get position => integer()();
  TextColumn get mediaId => text()();
  TextColumn get title => text().withDefault(const Constant(''))();

  /// Artists serialized as a JSON string array.
  TextColumn get artistsJson => text().withDefault(const Constant('[]'))();
  IntColumn get durationMs => integer().withDefault(const Constant(0))();
  TextColumn get source => text().withDefault(const Constant(''))();
  TextColumn get streamUrl => text().nullable()();
  DateTimeColumn get streamExpiresAt => dateTime().nullable()();
  TextColumn get mimeType => text().nullable()();
  DateTimeColumn get cachedAt => dateTime().withDefault(currentDateAndTime)();

  @override
  Set<Column> get primaryKey => {queueId, position};
}

/// Local play history, used to build a `LocalRadio` fallback queue when the
/// server is unreachable and the lookahead buffer is exhausted. `playCount` and
/// `lastPlayedAt` let the radio prefer recent + frequently played tracks.
class RecentPlays extends Table {
  TextColumn get mediaId => text()();
  TextColumn get title => text().withDefault(const Constant(''))();
  TextColumn get artistName => text().withDefault(const Constant(''))();
  TextColumn get source => text().withDefault(const Constant(''))();
  TextColumn get streamUrl => text().nullable()();
  IntColumn get durationMs => integer().withDefault(const Constant(0))();
  IntColumn get playCount => integer().withDefault(const Constant(1))();
  DateTimeColumn get lastPlayedAt =>
      dateTime().withDefault(currentDateAndTime)();

  @override
  Set<Column> get primaryKey => {mediaId};
}

/// Cold-start cache of the rendered Home feed (M5). One row holds the whole
/// `/home` JSON payload so a launch with the server unreachable can render
/// yesterday's sections with a "stale" indicator. Single logical row keyed by
/// the filters hash (so a prefs change doesn't show the wrong cached feed).
class HomeCache extends Table {
  TextColumn get cacheKey => text()();

  /// The full `/home` response JSON (sections + chips), stored verbatim.
  TextColumn get payloadJson => text()();
  DateTimeColumn get cachedAt => dateTime().withDefault(currentDateAndTime)();

  @override
  Set<Column> get primaryKey => {cacheKey};
}

/// A queued/active/completed download job (M6). The download manager runs these
/// in a background isolate; the row persists progress so a job resumes after an
/// app restart (Range-resumable via [receivedBytes]).
///
/// status: pending | running | completed | failed | canceled
class DownloadJobs extends Table {
  TextColumn get mediaId => text()();
  TextColumn get title => text().withDefault(const Constant(''))();

  /// The remote URL to fetch (server stream URL for local songs; resolved YT
  /// URL for remote, best-effort).
  TextColumn get sourceUrl => text()();
  TextColumn get status =>
      text().withDefault(const Constant('pending'))();
  IntColumn get totalBytes => integer().withDefault(const Constant(0))();
  IntColumn get receivedBytes => integer().withDefault(const Constant(0))();

  /// Optional playlist this job was enqueued for (per-playlist downloads).
  TextColumn get playlistId => text().nullable()();
  TextColumn get error => text().nullable()();
  DateTimeColumn get updatedAt =>
      dateTime().withDefault(currentDateAndTime)();

  @override
  Set<Column> get primaryKey => {mediaId};
}

/// A completed, locally-stored track (M6). The player's source resolver prefers
/// a row here over any `/next` URL so offline playback never touches the
/// network. [sha256] is the verified hash for local-library files (null for
/// best-effort YouTube downloads).
class DownloadedTracks extends Table {
  TextColumn get mediaId => text()();
  TextColumn get localPath => text()();
  IntColumn get bytes => integer().withDefault(const Constant(0))();
  TextColumn get sha256 => text().nullable()();
  DateTimeColumn get completedAt =>
      dateTime().withDefault(currentDateAndTime)();

  @override
  Set<Column> get primaryKey => {mediaId};
}

// ---------------------------------------------------------------------------
// Database
// ---------------------------------------------------------------------------

@DriftDatabase(
  tables: [
    LookaheadCache,
    RecentPlays,
    HomeCache,
    DownloadJobs,
    DownloadedTracks,
  ],
)
class SunflowerDatabase extends _$SunflowerDatabase {
  SunflowerDatabase() : super(_openConnection());

  /// Test seam: build a database over an in-memory or custom executor.
  SunflowerDatabase.forTesting(super.executor);

  @override
  int get schemaVersion => 1;

  // --- LookaheadCache -------------------------------------------------------

  /// Replaces the cached lookahead window for [queueId] with [items].
  Future<void> replaceLookahead(
    String queueId,
    List<LookaheadCacheCompanion> items,
  ) async {
    await transaction(() async {
      await (delete(
        lookaheadCache,
      )..where((t) => t.queueId.equals(queueId))).go();
      await batch((b) => b.insertAll(lookaheadCache, items));
    });
  }

  /// Returns the cached lookahead window for [queueId] ordered by position.
  Future<List<LookaheadCacheData>> cachedLookahead(String queueId) {
    return (select(lookaheadCache)
          ..where((t) => t.queueId.equals(queueId))
          ..orderBy([(t) => OrderingTerm(expression: t.position)]))
        .get();
  }

  // --- RecentPlays ----------------------------------------------------------

  /// Records a play of [mediaId], incrementing its play count and bumping the
  /// last-played timestamp. Upsert keyed on the media id.
  Future<void> recordPlay(RecentPlaysCompanion play) async {
    await into(recentPlays).insertOnConflictUpdate(play);
  }

  /// Returns up to [limit] recent plays, most recent first — the seed list for
  /// the offline local-radio fallback.
  Future<List<RecentPlay>> recentPlays_({int limit = 50}) {
    return (select(recentPlays)
          ..orderBy([
            (t) => OrderingTerm(
              expression: t.lastPlayedAt,
              mode: OrderingMode.desc,
            ),
          ])
          ..limit(limit))
        .get();
  }

  // --- HomeCache ------------------------------------------------------------

  /// Stores the rendered Home payload JSON for [cacheKey] (overwrites).
  Future<void> putHome(String cacheKey, String payloadJson) async {
    await into(homeCache).insertOnConflictUpdate(
      HomeCacheCompanion.insert(cacheKey: cacheKey, payloadJson: payloadJson),
    );
  }

  /// Returns the cached Home payload for [cacheKey], or null on a miss.
  Future<HomeCacheData?> getHome(String cacheKey) {
    return (select(homeCache)
          ..where((t) => t.cacheKey.equals(cacheKey))
          ..limit(1))
        .getSingleOrNull();
  }

  // --- Downloads ------------------------------------------------------------

  /// Inserts or updates a download job (keyed on media id).
  Future<void> upsertJob(DownloadJobsCompanion job) async {
    await into(downloadJobs).insertOnConflictUpdate(job);
  }

  /// Updates progress for an in-flight job.
  Future<void> updateJobProgress(
    String mediaId, {
    required int received,
    required int total,
    String? status,
  }) async {
    await (update(downloadJobs)..where((t) => t.mediaId.equals(mediaId))).write(
      DownloadJobsCompanion(
        receivedBytes: Value(received),
        totalBytes: Value(total),
        status: status == null ? const Value.absent() : Value(status),
        updatedAt: Value(DateTime.now()),
      ),
    );
  }

  /// Marks a job failed with an error message.
  Future<void> failJob(String mediaId, String error) async {
    await (update(downloadJobs)..where((t) => t.mediaId.equals(mediaId))).write(
      DownloadJobsCompanion(
        status: const Value('failed'),
        error: Value(error),
        updatedAt: Value(DateTime.now()),
      ),
    );
  }

  /// Removes a job row (on cancel or after completion).
  Future<void> deleteJob(String mediaId) async {
    await (delete(downloadJobs)..where((t) => t.mediaId.equals(mediaId))).go();
  }

  /// All jobs not yet completed, oldest first — the resume queue on app start.
  Future<List<DownloadJob>> pendingJobs() {
    return (select(downloadJobs)
          ..where((t) => t.status.isNotIn(['completed']))
          ..orderBy([(t) => OrderingTerm(expression: t.updatedAt)]))
        .get();
  }

  /// Watches all jobs for the downloads UI.
  Stream<List<DownloadJob>> watchJobs() {
    return (select(downloadJobs)
          ..orderBy([(t) => OrderingTerm(expression: t.updatedAt)]))
        .watch();
  }

  /// Records a completed local download (and removes its job row).
  Future<void> completeDownload(DownloadedTracksCompanion track) async {
    await transaction(() async {
      await into(downloadedTracks).insertOnConflictUpdate(track);
      final mediaId = track.mediaId.value;
      await (update(downloadJobs)..where((t) => t.mediaId.equals(mediaId))).write(
        const DownloadJobsCompanion(status: Value('completed')),
      );
    });
  }

  /// Returns the downloaded track for [mediaId], or null if not downloaded.
  Future<DownloadedTrack?> downloadedTrack(String mediaId) {
    return (select(downloadedTracks)
          ..where((t) => t.mediaId.equals(mediaId))
          ..limit(1))
        .getSingleOrNull();
  }

  /// True if [mediaId] is available locally.
  Future<bool> isDownloaded(String mediaId) async {
    return (await downloadedTrack(mediaId)) != null;
  }

  /// Removes a downloaded track record (after the file is deleted).
  Future<void> removeDownloadedTrack(String mediaId) async {
    await (delete(downloadedTracks)..where((t) => t.mediaId.equals(mediaId)))
        .go();
  }
}

LazyDatabase _openConnection() {
  return LazyDatabase(() async {
    final dir = await getApplicationSupportDirectory();
    final file = File(p.join(dir.path, 'sunflower.sqlite'));
    return NativeDatabase.createInBackground(file);
  });
}
