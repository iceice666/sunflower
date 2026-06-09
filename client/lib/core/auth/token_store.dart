import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';

const _kServerUrl = 'sunflower_server_url';
const _kToken = 'sunflower_token';

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

/// Persists [serverUrl] and [token] to secure storage, then
/// invalidates [tokenProvider] so [SunflowerApp] re-routes to the library.
Future<void> saveCredentials(
  WidgetRef ref,
  String serverUrl,
  String token,
) async {
  await Future.wait([
    _storage.write(key: _kServerUrl, value: serverUrl),
    _storage.write(key: _kToken, value: token),
  ]);
  ref.invalidate(tokenProvider);
  ref.invalidate(serverUrlProvider);
}

/// Clears all stored credentials (for future logout / re-register flow).
Future<void> clearCredentials() async {
  await Future.wait([
    _storage.delete(key: _kServerUrl),
    _storage.delete(key: _kToken),
  ]);
}
