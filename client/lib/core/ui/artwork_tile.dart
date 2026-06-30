import 'package:cached_network_image/cached_network_image.dart';
import 'package:flutter/material.dart';

import 'sunflower_theme.dart';

class ArtworkTile extends StatelessWidget {
  const ArtworkTile({
    super.key,
    this.imageUrl,
    this.httpHeaders,
    this.size = 56,
    this.radius = 8,
    this.icon = Icons.music_note,
    this.shape = BoxShape.rectangle,
  });

  final String? imageUrl;
  final Map<String, String>? httpHeaders;
  final double size;
  final double radius;
  final IconData icon;
  final BoxShape shape;

  @override
  Widget build(BuildContext context) {
    final url = imageUrl;
    final borderRadius =
        shape == BoxShape.circle ? null : BorderRadius.circular(radius);
    final placeholder = _Placeholder(size: size, icon: icon, shape: shape);
    if (url == null || url.isEmpty) {
      return placeholder;
    }
    return ClipRRect(
      borderRadius: borderRadius ?? BorderRadius.circular(size / 2),
      child: CachedNetworkImage(
        imageUrl: url,
        httpHeaders: httpHeaders,
        width: size,
        height: size,
        fit: BoxFit.cover,
        placeholder: (_, __) => placeholder,
        errorWidget: (_, __, ___) => placeholder,
      ),
    );
  }
}

class _Placeholder extends StatelessWidget {
  const _Placeholder({
    required this.size,
    required this.icon,
    required this.shape,
  });

  final double size;
  final IconData icon;
  final BoxShape shape;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: size,
      height: size,
      decoration: BoxDecoration(
        color: SunflowerColors.surfaceHigh,
        shape: shape,
        borderRadius:
            shape == BoxShape.circle ? null : BorderRadius.circular(8),
        border: Border.all(color: SunflowerColors.outline),
      ),
      child: Icon(icon, size: size * 0.42, color: Colors.white70),
    );
  }
}
