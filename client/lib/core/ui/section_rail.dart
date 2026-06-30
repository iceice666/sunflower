import 'package:flutter/material.dart';

import '../../core/api/sunflower_api.dart';
import 'artwork_tile.dart';
import 'media_actions.dart';

class SectionRail extends StatelessWidget {
  const SectionRail({
    super.key,
    required this.section,
    required this.onTap,
  });

  final HomeSection section;
  final void Function(HomeSection section, HomeItem item, int index) onTap;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 18),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 8, 16, 10),
            child: Row(
              children: [
                Expanded(
                  child: Text(
                    section.title,
                    style: Theme.of(context).textTheme.titleLarge?.copyWith(
                          fontWeight: FontWeight.w800,
                        ),
                  ),
                ),
                const Icon(Icons.chevron_right, size: 22),
              ],
            ),
          ),
          SizedBox(
            height: 232,
            child: ListView.separated(
              scrollDirection: Axis.horizontal,
              padding: const EdgeInsets.symmetric(horizontal: 16),
              itemCount: section.items.length,
              separatorBuilder: (_, __) => const SizedBox(width: 14),
              itemBuilder: (context, i) {
                final item = section.items[i];
                return _RailTile(
                  item: item,
                  onTap: () => onTap(section, item, i),
                );
              },
            ),
          ),
        ],
      ),
    );
  }
}

class _RailTile extends StatelessWidget {
  const _RailTile({required this.item, required this.onTap});

  final HomeItem item;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: 148,
      child: InkWell(
        borderRadius: BorderRadius.circular(10),
        onTap: onTap,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Stack(
              children: [
                ArtworkTile(imageUrl: item.thumbnailUrl, size: 148, radius: 10),
                Positioned(
                  right: 8,
                  bottom: 8,
                  child: Container(
                    decoration: BoxDecoration(
                      color: Colors.black.withValues(alpha: 0.64),
                      shape: BoxShape.circle,
                    ),
                    child: const Icon(Icons.play_arrow, size: 30),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 8),
            Text(
              item.title,
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
              style: Theme.of(context)
                  .textTheme
                  .bodyMedium
                  ?.copyWith(fontWeight: FontWeight.w700),
            ),
            const SizedBox(height: 2),
            Row(
              children: [
                Expanded(
                  child: Text(
                    item.artists.isEmpty
                        ? item.source
                        : item.artists.join(', '),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: Theme.of(context).textTheme.bodySmall,
                  ),
                ),
                LikeButton(mediaId: item.mediaId, compact: true),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
