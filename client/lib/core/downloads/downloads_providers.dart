import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../api/sunflower_api.dart';
import '../auth/token_store.dart';
import '../db/database.dart';
import '../db/database_provider.dart';
import '../player/source_resolver.dart';
import '../sync/sync_providers.dart';
import 'download_manager.dart';

/// The device id is stored at registration time. M6 reads it to scope the
/// per-device download registry. Falls back to empty (server call then no-ops).
final deviceIdProvider = FutureProvider<String>((ref) async {
  // device_id is persisted alongside the token at registration (token_store).
  return (await readDeviceId()) ?? '';
});

/// Singleton [DownloadManager]. Started lazily; disposed with the provider.
final downloadManagerProvider = Provider<DownloadManager>((ref) {
  final api = ref.watch(sunflowerApiProvider);
  final bufferedApi = ref.watch(bufferedApiProvider);
  final db = ref.watch(databaseProvider);
  final deviceId = ref.watch(deviceIdProvider).valueOrNull ?? '';
  final mgr = DownloadManager(
    api: api,
    bufferedApi: bufferedApi,
    db: db,
    deviceId: deviceId,
  );
  ref.onDispose(mgr.dispose);
  return mgr;
});

/// Live stream of all download jobs for the downloads UI.
final downloadJobsProvider = StreamProvider<List<DownloadJob>>((ref) {
  return ref.watch(downloadManagerProvider).watchJobs();
});

/// Prefer-local source resolver for the player layer.
final sourceResolverProvider = Provider<SourceResolver>((ref) {
  return SourceResolver(ref.watch(databaseProvider));
});
