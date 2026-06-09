import 'dart:io';

/// Platform-capability matrix for the Sunflower player.
///
/// All platform-specific branches MUST go through this class. No bare
/// Platform.isAndroid / Platform.isIOS checks in UI or handler code.
/// This makes M9 platform porting explicit and auditable.
abstract final class PlayerCapabilities {
  /// True if the current platform supports background audio playback via
  /// audio_service foreground services / background modes.
  static bool get backgroundAudio =>
      Platform.isAndroid || Platform.isIOS;

  /// True if the platform provides OS media session controls (lock screen,
  /// notification shade, Bluetooth head-set buttons).
  static bool get mediaSession =>
      Platform.isAndroid || Platform.isIOS;

  /// True if the platform supports downloading tracks to local storage for
  /// offline playback (M6).
  static bool get offlineDownloads =>
      Platform.isAndroid || Platform.isIOS;

  /// True if range requests (HTTP 206 / seek) are expected to work.
  /// ExoPlayer on Android handles range natively; AVPlayer on iOS too.
  /// Desktop/web targets may behave differently.
  static bool get rangeSeek =>
      Platform.isAndroid || Platform.isIOS;
}
