import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/db/database.dart';
import '../../core/downloads/downloads_providers.dart';

/// Lists active and completed downloads (M6). Active jobs show progress and a
/// cancel action; completed jobs show a remove action.
class DownloadsScreen extends ConsumerWidget {
  const DownloadsScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final jobsAsync = ref.watch(downloadJobsProvider);

    return Scaffold(
      appBar: AppBar(title: const Text('Downloads')),
      body: jobsAsync.when(
        data: (jobs) {
          if (jobs.isEmpty) {
            return const Center(child: Text('No downloads'));
          }
          final active = jobs.where((j) => j.status != 'completed').toList();
          final done = jobs.where((j) => j.status == 'completed').toList();
          return ListView(
            children: [
              if (active.isNotEmpty) const _Header('Active'),
              for (final j in active) _ActiveTile(job: j),
              if (done.isNotEmpty) const _Header('Downloaded'),
              for (final j in done) _DoneTile(job: j),
            ],
          );
        },
        loading: () => const Center(child: CircularProgressIndicator()),
        error: (e, _) => const Center(child: Text('Could not load downloads')),
      ),
    );
  }
}

class _Header extends StatelessWidget {
  const _Header(this.text);
  final String text;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 16, 16, 4),
      child: Text(
        text,
        style: Theme.of(context).textTheme.labelLarge?.copyWith(
              color: Theme.of(context).colorScheme.primary,
            ),
      ),
    );
  }
}

class _ActiveTile extends ConsumerWidget {
  const _ActiveTile({required this.job});
  final DownloadJob job;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final progress =
        job.totalBytes > 0 ? job.receivedBytes / job.totalBytes : null;
    final failed = job.status == 'failed';
    return ListTile(
      title: Text(
        job.title.isEmpty ? job.mediaId : job.title,
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
      ),
      subtitle: failed
          ? Text(job.error ?? 'Failed',
              style: TextStyle(color: Theme.of(context).colorScheme.error))
          : LinearProgressIndicator(value: progress),
      trailing: IconButton(
        icon: const Icon(Icons.close),
        onPressed: () => ref.read(downloadManagerProvider).cancel(job.mediaId),
      ),
    );
  }
}

class _DoneTile extends ConsumerWidget {
  const _DoneTile({required this.job});
  final DownloadJob job;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return ListTile(
      leading: const Icon(Icons.download_done),
      title: Text(
        job.title.isEmpty ? job.mediaId : job.title,
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
      ),
      trailing: IconButton(
        icon: const Icon(Icons.delete_outline),
        onPressed: () => ref.read(downloadManagerProvider).remove(job.mediaId),
      ),
    );
  }
}
