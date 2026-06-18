import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'database.dart';

/// Singleton [SunflowerDatabase] for the app. Opened lazily on first read and
/// kept for the process lifetime (Drift manages its own connection pool).
final databaseProvider = Provider<SunflowerDatabase>((ref) {
  final db = SunflowerDatabase();
  ref.onDispose(db.close);
  return db;
});
