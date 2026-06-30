import 'package:flutter/material.dart';

import 'artwork_tile.dart';
import 'media_actions.dart';

class TrackRow extends StatelessWidget {
  const TrackRow({
    super.key,
    required this.mediaId,
    required this.title,
    this.subtitle = '',
    this.thumbnailUrl,
    this.httpHeaders,
    this.onTap,
    this.leadingIcon = Icons.music_note,
    this.showActions = true,
  });

  final String mediaId;
  final String title;
  final String subtitle;
  final String? thumbnailUrl;
  final Map<String, String>? httpHeaders;
  final VoidCallback? onTap;
  final IconData leadingIcon;
  final bool showActions;

  @override
  Widget build(BuildContext context) {
    return ListTile(
      contentPadding: const EdgeInsets.symmetric(horizontal: 16, vertical: 2),
      leading: ArtworkTile(
        imageUrl: thumbnailUrl,
        httpHeaders: httpHeaders,
        size: 50,
        radius: 8,
        icon: leadingIcon,
      ),
      title: Text(title, maxLines: 1, overflow: TextOverflow.ellipsis),
      subtitle: subtitle.isEmpty
          ? null
          : Text(subtitle, maxLines: 1, overflow: TextOverflow.ellipsis),
      trailing: showActions
          ? Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                LikeButton(mediaId: mediaId, compact: true),
                MediaOverflowMenu(mediaId: mediaId, title: title),
              ],
            )
          : null,
      onTap: onTap,
    );
  }
}
