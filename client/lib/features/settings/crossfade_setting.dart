import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:shared_preferences/shared_preferences.dart';

const _kCrossfadeEnabled = 'crossfade_enabled';
const _kCrossfadeSeconds = 'crossfade_seconds';

/// Crossfade configuration (M8 optional polish): on/off + fade duration.
class CrossfadeConfig {
  const CrossfadeConfig({this.enabled = false, this.seconds = 6});
  final bool enabled;
  final int seconds;

  CrossfadeConfig copyWith({bool? enabled, int? seconds}) => CrossfadeConfig(
        enabled: enabled ?? this.enabled,
        seconds: seconds ?? this.seconds,
      );
}

/// Persisted crossfade config, backed by shared_preferences.
class CrossfadeController extends StateNotifier<CrossfadeConfig> {
  CrossfadeController() : super(const CrossfadeConfig()) {
    _load();
  }

  Future<void> _load() async {
    final prefs = await SharedPreferences.getInstance();
    state = CrossfadeConfig(
      enabled: prefs.getBool(_kCrossfadeEnabled) ?? false,
      seconds: prefs.getInt(_kCrossfadeSeconds) ?? 6,
    );
  }

  Future<void> setEnabled(bool v) async {
    state = state.copyWith(enabled: v);
    final prefs = await SharedPreferences.getInstance();
    await prefs.setBool(_kCrossfadeEnabled, v);
  }

  Future<void> setSeconds(int s) async {
    state = state.copyWith(seconds: s);
    final prefs = await SharedPreferences.getInstance();
    await prefs.setInt(_kCrossfadeSeconds, s);
  }
}

final crossfadeProvider =
    StateNotifierProvider<CrossfadeController, CrossfadeConfig>(
  (ref) => CrossfadeController(),
);

/// Settings UI: a toggle plus a duration slider (1–12 s), enabled only when the
/// toggle is on.
class CrossfadeSetting extends ConsumerWidget {
  const CrossfadeSetting({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final cfg = ref.watch(crossfadeProvider);
    final ctrl = ref.read(crossfadeProvider.notifier);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        SwitchListTile(
          title: const Text('Crossfade'),
          subtitle: const Text('Blend the end of one track into the next'),
          value: cfg.enabled,
          onChanged: ctrl.setEnabled,
        ),
        if (cfg.enabled)
          ListTile(
            title: Text('Duration: ${cfg.seconds}s'),
            subtitle: Slider(
              min: 1,
              max: 12,
              divisions: 11,
              value: cfg.seconds.toDouble(),
              label: '${cfg.seconds}s',
              onChanged: (v) => ctrl.setSeconds(v.round()),
            ),
          ),
      ],
    );
  }
}
