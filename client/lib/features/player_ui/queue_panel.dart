import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/player/player_bootstrap.dart';

/// Shows the upcoming items in the active queue (M4). Reads the player's
/// in-memory sequence so it reflects lookahead + local-radio fallback entries
/// as they are appended. Tap an item to jump to it.
class QueuePanel extends ConsumerWidget {
  const QueuePanel({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final handler = ref.watch(audioHandlerProvider);
    final current = ref.watch(currentMediaItemProvider).valueOrNull;

    final upcoming = handler.upcomingQueue;
    if (upcoming.isEmpty) {
      return const Center(child: Text('Nothing up next'));
    }

    return ListView.builder(
      itemCount: upcoming.length,
      itemBuilder: (context, i) {
        final entry = upcoming[i];
        final item = entry.item;
        final isCurrent = current?.id == item.id;
        return ListTile(
          dense: true,
          leading: Icon(
            isCurrent ? Icons.play_arrow : Icons.music_note,
            color: isCurrent ? Theme.of(context).colorScheme.primary : null,
          ),
          title: Text(item.title, maxLines: 1, overflow: TextOverflow.ellipsis),
          subtitle: item.artist == null
              ? null
              : Text(
                  item.artist!,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                ),
          onTap: () => handler.skipToQueueItem(entry.queueIndex),
        );
      },
    );
  }
}
