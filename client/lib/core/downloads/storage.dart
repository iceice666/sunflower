import 'dart:io';

import 'package:path/path.dart' as p;
import 'package:path_provider/path_provider.dart';

/// Resolves on-disk locations for offline downloads and performs free-space
/// checks. All downloads live under
/// `getApplicationSupportDirectory()/downloads/`.
class DownloadStorage {
  /// Returns the downloads directory, creating it if needed.
  Future<Directory> downloadsDir() async {
    final base = await getApplicationSupportDirectory();
    final dir = Directory(p.join(base.path, 'downloads'));
    if (!await dir.exists()) {
      await dir.create(recursive: true);
    }
    return dir;
  }

  /// Returns the target file path for [mediaId]. The media id's ':' is replaced
  /// so it is a valid filename on all platforms; [extension] defaults to a
  /// generic container.
  Future<String> pathFor(String mediaId, {String extension = 'audio'}) async {
    final dir = await downloadsDir();
    final safe = mediaId.replaceAll(':', '_').replaceAll('/', '_');
    return p.join(dir.path, '$safe.$extension');
  }

  /// The partial-download path (Range-resumable). The worker writes here and
  /// renames to the final path on completion so a crashed download never leaves
  /// a truncated "complete" file.
  Future<String> partialPathFor(String mediaId,
      {String extension = 'audio'}) async {
    return '${await pathFor(mediaId, extension: extension)}.part';
  }

  // Disk-full handling: Dart core exposes no portable free-space API, so we do
  // not pre-check. The worker writes to the `.part` file in a streaming sink;
  // an out-of-space write throws (ENOSPC), which the manager records as a failed
  // job with the error surfaced in the downloads UI (M6 acceptance).

  /// Deletes the final local file for [mediaId] if present.
  Future<void> delete(String mediaId, {String extension = 'audio'}) async {
    final f = File(await pathFor(mediaId, extension: extension));
    if (await f.exists()) {
      await f.delete();
    }
  }

  /// Deletes the in-progress `.part` file for [mediaId] if present (on cancel).
  Future<void> deletePartial(String mediaId,
      {String extension = 'audio'}) async {
    final f = File(await partialPathFor(mediaId, extension: extension));
    if (await f.exists()) {
      await f.delete();
    }
  }
}
