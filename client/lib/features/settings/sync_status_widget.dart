import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/sync/sync_providers.dart';

/// Shows the count of pending offline mutations and a "retry now" action
/// (M7). Renders nothing when the buffer is drained (count 0).
class SyncStatusWidget extends ConsumerWidget {
  const SyncStatusWidget({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final pending = ref.watch(pendingCountProvider).valueOrNull ?? 0;
    final drops = ref.watch(bufferedApiProvider).overflowDrops;

    if (pending == 0 && drops == 0) return const SizedBox.shrink();

    return Card(
      margin: const EdgeInsets.all(8),
      child: ListTile(
        leading: const Icon(Icons.sync),
        title: Text('$pending pending'),
        subtitle: drops > 0
            ? Text('$drops dropped (buffer full)',
                style: TextStyle(color: Theme.of(context).colorScheme.error))
            : null,
        trailing: TextButton(
          onPressed: () => ref.read(bufferedApiProvider).retryNow(),
          child: const Text('Retry now'),
        ),
      ),
    );
  }
}
