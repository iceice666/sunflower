import 'package:flutter/material.dart';

/// Horizontal mood/genre chip bar (Relax, Workout, Sleep, …) from the YouTube
/// Music home feed. In v1 chips are display-only affordances; tapping a chip is
/// a future filter hook.
class ChipBar extends StatelessWidget {
  const ChipBar({super.key, required this.chips, this.onTap});

  final List<String> chips;
  final void Function(String chip)? onTap;

  @override
  Widget build(BuildContext context) {
    if (chips.isEmpty) return const SizedBox.shrink();
    return SizedBox(
      height: 44,
      child: ListView.separated(
        scrollDirection: Axis.horizontal,
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 4),
        itemCount: chips.length,
        separatorBuilder: (_, __) => const SizedBox(width: 8),
        itemBuilder: (context, i) {
          final chip = chips[i];
          return ActionChip(
            label: Text(chip),
            onPressed: onTap == null ? null : () => onTap!(chip),
          );
        },
      ),
    );
  }
}
