import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';

const _kServerUrl = 'sunflower_server_url';
const _kToken = 'sunflower_token';
const _kDeviceId = 'sunflower_device_id';

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

/// Persists [serverUrl], [token], and [deviceId] to secure storage, then
/// invalidates [tokenProvider] so [SunflowerApp] re-routes to the library.
Future<void> saveCredentials(
  WidgetRef ref,
  String serverUrl,
  String token, {
  String deviceId = '',
}) async {
  await Future.wait([
    _storage.write(key: _kServerUrl, value: serverUrl),
    _storage.write(key: _kToken, value: token),
    _storage.write(key: _kDeviceId, value: deviceId),
  ]);
  ref.invalidate(tokenProvider);
  ref.invalidate(serverUrlProvider);
}

/// Reads the stored device id (M6 download registry scope), or null.
Future<String?> readDeviceId() => _storage.read(key: _kDeviceId);

/// Clears all stored credentials (for future logout / re-register flow).
Future<void> clearCredentials() async {
  await Future.wait([
    _storage.delete(key: _kServerUrl),
    _storage.delete(key: _kToken),
    _storage.delete(key: _kDeviceId),
  ]);
}
