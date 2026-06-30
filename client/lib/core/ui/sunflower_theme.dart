import 'package:flutter/material.dart';

abstract final class SunflowerColors {
  static const black = Color(0xFF080704);
  static const surface = Color(0xFF11100C);
  static const surfaceHigh = Color(0xFF1B1710);
  static const outline = Color(0xFF3B3326);
  static const amber = Color(0xFFFFB300);
  static const orange = Color(0xFFFF6D2D);
}

ThemeData sunflowerTheme() {
  final scheme = ColorScheme.fromSeed(
    seedColor: SunflowerColors.amber,
    brightness: Brightness.dark,
  ).copyWith(
    primary: SunflowerColors.amber,
    secondary: SunflowerColors.orange,
    surface: SunflowerColors.surface,
    surfaceContainer: SunflowerColors.surface,
    surfaceContainerHigh: SunflowerColors.surfaceHigh,
    surfaceContainerHighest: const Color(0xFF241E15),
    outline: SunflowerColors.outline,
  );

  return ThemeData(
    colorScheme: scheme,
    scaffoldBackgroundColor: SunflowerColors.black,
    canvasColor: SunflowerColors.black,
    useMaterial3: true,
    appBarTheme: const AppBarTheme(
      backgroundColor: SunflowerColors.black,
      foregroundColor: Colors.white,
      elevation: 0,
      centerTitle: false,
    ),
    navigationBarTheme: NavigationBarThemeData(
      backgroundColor: SunflowerColors.surface,
      indicatorColor: SunflowerColors.amber.withValues(alpha: 0.18),
      labelTextStyle: WidgetStateProperty.resolveWith(
        (states) => TextStyle(
          fontSize: 12,
          fontWeight:
              states.contains(WidgetState.selected) ? FontWeight.w700 : null,
        ),
      ),
    ),
    chipTheme: ChipThemeData(
      backgroundColor: SunflowerColors.surfaceHigh,
      selectedColor: SunflowerColors.amber.withValues(alpha: 0.18),
      disabledColor: SunflowerColors.surfaceHigh,
      side: const BorderSide(color: SunflowerColors.outline),
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(10)),
    ),
    sliderTheme: SliderThemeData(
      activeTrackColor: SunflowerColors.amber,
      thumbColor: SunflowerColors.amber,
      inactiveTrackColor: Colors.white.withValues(alpha: 0.16),
    ),
    filledButtonTheme: FilledButtonThemeData(
      style: FilledButton.styleFrom(
        backgroundColor: SunflowerColors.amber,
        foregroundColor: Colors.black,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(18)),
      ),
    ),
    inputDecorationTheme: InputDecorationTheme(
      filled: true,
      fillColor: SunflowerColors.surface,
      border: OutlineInputBorder(
        borderRadius: BorderRadius.circular(14),
        borderSide: const BorderSide(color: SunflowerColors.outline),
      ),
      enabledBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(14),
        borderSide: const BorderSide(color: SunflowerColors.outline),
      ),
      focusedBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(14),
        borderSide: const BorderSide(color: SunflowerColors.amber),
      ),
    ),
  );
}
