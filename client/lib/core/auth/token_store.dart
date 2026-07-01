import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';

const _kServerUrl = 'sunflower_server_url';
const _kRecommendationServerUrl = 'sunflower_recommendation_server_url';
const _kToken = 'sunflower_token';
const _kDeviceId = 'sunflower_device_id';
const _defaultRecommendationServerUrl = String.fromEnvironment(
  'SUNFLOWER_RECOMMENDATION_URL',
);

const _storage = FlutterSecureStorage(
  aOptions: AndroidOptions(encryptedSharedPreferences: true),
);

/// Reads the stored [token] from secure storage.
/// Returns null when no token exists (first launch / after logout).
final tokenProvider = FutureProvider<String?>((ref) async {
  return _storage.read(key: _kToken);
});

/// Reads the stored server base URL (e.g. "http://192.168.1.10:8080").
final serverUrlProvider = FutureProvider<String?>((ref) async {
  return _storage.read(key: _kServerUrl);
});

/// Reads the optional standalone recommendation server base URL.
///
/// When absent, recommendation reads and feedback use [serverUrlProvider].
final recommendationServerUrlProvider = FutureProvider<String?>((ref) async {
  final stored = _normalServerUrl(
    await _storage.read(key: _kRecommendationServerUrl),
  );
  return stored ?? _normalServerUrl(_defaultRecommendationServerUrl);
});

/// Effective recommendation base URL: standalone rec server if configured,
/// otherwise the main Sunflower server.
final recommendationBaseUrlProvider = Provider<String>((ref) {
  final recommendationUrl =
      ref.watch(recommendationServerUrlProvider).valueOrNull;
  if (recommendationUrl != null && recommendationUrl.isNotEmpty) {
    return recommendationUrl;
  }
  return ref.watch(serverUrlProvider).valueOrNull ?? '';
});

/// Persists [serverUrl], [token], and [deviceId] to secure storage, then
/// invalidates [tokenProvider] so [SunflowerApp] re-routes to the library.
Future<void> saveCredentials(
  WidgetRef ref,
  String serverUrl,
  String token, {
  String deviceId = '',
  String? recommendationServerUrl,
}) async {
  final normalizedRecommendationUrl = _normalServerUrl(recommendationServerUrl);
  await Future.wait([
    _storage.write(key: _kServerUrl, value: serverUrl),
    _storage.write(key: _kToken, value: token),
    _storage.write(key: _kDeviceId, value: deviceId),
    if (normalizedRecommendationUrl == null)
      _storage.delete(key: _kRecommendationServerUrl)
    else
      _storage.write(
        key: _kRecommendationServerUrl,
        value: normalizedRecommendationUrl,
      ),
  ]);
  ref.invalidate(tokenProvider);
  ref.invalidate(serverUrlProvider);
  ref.invalidate(recommendationServerUrlProvider);
  ref.invalidate(recommendationBaseUrlProvider);
}

/// Reads the stored device id (M6 download registry scope), or null.
Future<String?> readDeviceId() => _storage.read(key: _kDeviceId);

/// Persists or clears the optional standalone recommendation server URL.
Future<void> saveRecommendationServerUrl(
  WidgetRef ref,
  String? recommendationServerUrl,
) async {
  final normalizedRecommendationUrl = _normalServerUrl(recommendationServerUrl);
  if (normalizedRecommendationUrl == null) {
    await _storage.delete(key: _kRecommendationServerUrl);
  } else {
    await _storage.write(
      key: _kRecommendationServerUrl,
      value: normalizedRecommendationUrl,
    );
  }
  ref.invalidate(recommendationServerUrlProvider);
  ref.invalidate(recommendationBaseUrlProvider);
}

/// Clears all stored credentials (for future logout / re-register flow).
Future<void> clearCredentials() async {
  await Future.wait([
    _storage.delete(key: _kServerUrl),
    _storage.delete(key: _kRecommendationServerUrl),
    _storage.delete(key: _kToken),
    _storage.delete(key: _kDeviceId),
  ]);
}

Future<void> clearCredentialsAndNotify(Ref ref) async {
  await clearCredentials();
  ref.invalidate(tokenProvider);
  ref.invalidate(serverUrlProvider);
  ref.invalidate(recommendationServerUrlProvider);
  ref.invalidate(recommendationBaseUrlProvider);
}

String? _normalServerUrl(String? value) {
  final trimmed = value?.trim().replaceAll(RegExp(r'/+$'), '');
  if (trimmed == null || trimmed.isEmpty) return null;
  return trimmed;
}
