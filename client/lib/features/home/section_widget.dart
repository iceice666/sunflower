import 'package:cached_network_image/cached_network_image.dart';
import 'package:flutter/material.dart';

import '../../core/api/sunflower_api.dart';

/// A horizontal recommendation row: a title plus a scrollable strip of tiles.
/// Tapping a tile invokes [onTap] (start playback from that item).
class SectionWidget extends StatelessWidget {
  const SectionWidget({super.key, required this.section, required this.onTap});

  final HomeSection section;
  final void Function(HomeSection section, HomeItem item, int index) onTap;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 16, 16, 8),
          child: Text(
            section.title,
            style: Theme.of(
              context,
            ).textTheme.titleMedium?.copyWith(fontWeight: FontWeight.bold),
          ),
        ),
        SizedBox(
          height: 200,
          child: ListView.separated(
            scrollDirection: Axis.horizontal,
            padding: const EdgeInsets.symmetric(horizontal: 16),
            itemCount: section.items.length,
            separatorBuilder: (_, __) => const SizedBox(width: 12),
            itemBuilder: (context, i) {
              final item = section.items[i];
              return _Tile(item: item, onTap: () => onTap(section, item, i));
            },
          ),
        ),
      ],
    );
  }
}

class _Tile extends StatelessWidget {
  const _Tile({required this.item, required this.onTap});

  final HomeItem item;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return InkWell(
      onTap: onTap,
      child: SizedBox(
        width: 140,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            ClipRRect(
              borderRadius: BorderRadius.circular(8),
              child: _artwork(),
            ),
            const SizedBox(height: 6),
            Text(
              item.title,
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
              style: Theme.of(context).textTheme.bodyMedium,
            ),
            if (item.artists.isNotEmpty)
              Text(
                item.artists.join(', '),
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: Theme.of(context).textTheme.bodySmall,
              ),
          ],
        ),
      ),
    );
  }

  Widget _artwork() {
    final url = item.thumbnailUrl;
    if (url == null || url.isEmpty) {
      return Container(
        width: 140,
        height: 140,
        color: Colors.grey[800],
        child: const Icon(Icons.music_note, size: 48),
      );
    }
    return CachedNetworkImage(
      imageUrl: url,
      width: 140,
      height: 140,
      fit: BoxFit.cover,
      placeholder: (_, __) =>
          Container(width: 140, height: 140, color: Colors.grey[850]),
      errorWidget: (_, __, ___) => Container(
        width: 140,
        height: 140,
        color: Colors.grey[800],
        child: const Icon(Icons.music_note, size: 48),
      ),
    );
  }
}
