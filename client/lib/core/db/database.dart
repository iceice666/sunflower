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

// ---------------------------------------------------------------------------
// Database
// ---------------------------------------------------------------------------

@DriftDatabase(tables: [LookaheadCache, RecentPlays, HomeCache])
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
}

LazyDatabase _openConnection() {
  return LazyDatabase(() async {
    final dir = await getApplicationSupportDirectory();
    final file = File(p.join(dir.path, 'sunflower.sqlite'));
    return NativeDatabase.createInBackground(file);
  });
}
