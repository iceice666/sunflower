import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/api/sunflower_api.dart';
import '../../core/db/database.dart';
import '../../core/downloads/downloads_providers.dart';
import '../../core/player/capabilities.dart';

/// Reusable per-track / per-playlist download affordance. Shows a download icon,
/// a progress spinner while in flight, and a check when complete. On
/// unsupported platforms (web) it renders a disabled "not supported" icon per
/// the M6 platform notes.
class DownloadButton extends ConsumerWidget {
  const DownloadButton({
    super.key,
    required this.mediaId,
    required this.title,
    required this.streamUrl,
  });

  final String mediaId;
  final String title;
  final String streamUrl;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    if (!PlayerCapabilities.offlineDownloads) {
      return const Tooltip(
        message: 'Downloads not supported on this platform',
        child: Icon(Icons.cloud_off, size: 20),
      );
    }

    final jobsAsync = ref.watch(downloadJobsProvider);
    final jobs = jobsAsync.valueOrNull ?? const <DownloadJob>[];
    DownloadJob? job;
    for (final j in jobs) {
      if (j.mediaId == mediaId) {
        job = j;
        break;
      }
    }

    if (job != null && job.status == 'running') {
      final progress =
          job.totalBytes > 0 ? job.receivedBytes / job.totalBytes : null;
      return SizedBox(
        width: 24,
        height: 24,
        child: CircularProgressIndicator(strokeWidth: 2, value: progress),
      );
    }
    if (job != null && job.status == 'completed') {
      return const Icon(Icons.download_done, size: 20);
    }
    if (job != null && job.status == 'failed') {
      return IconButton(
        icon: const Icon(Icons.error_outline, size: 20),
        tooltip: job.error ?? 'Download failed',
        onPressed: () => _enqueue(ref),
      );
    }

    return IconButton(
      icon: const Icon(Icons.download_outlined, size: 20),
      onPressed: () => _enqueue(ref),
    );
  }

  Future<void> _enqueue(WidgetRef ref) async {
    final mgr = ref.read(downloadManagerProvider);
    await mgr.start();
    await mgr.enqueueTrack(
      mediaId: mediaId,
      title: title,
      streamUrl: streamUrl,
    );
  }
}

/// Convenience for building a track's stream URL from the API (local songs).
String trackStreamUrl(SunflowerApi api, String mediaId) =>
    api.streamUrl(mediaId);
