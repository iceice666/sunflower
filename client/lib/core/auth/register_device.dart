import 'dart:io';

import 'package:dio/dio.dart';
import 'package:uuid/uuid.dart';

/// Result of device registration: the opaque bearer token and the server's
/// device id (needed for the M6 per-device download registry).
class RegisterResult {
  const RegisterResult({required this.token, required this.deviceId});
  final String token;
  final String deviceId;
}

/// Registers this device with the Sunflower server at [baseUrl] and returns the
/// opaque bearer token plus the assigned device id.
///
/// Throws [DioException] on network errors, [StateError] if the server returns
/// a success status but the body has no token field.
Future<RegisterResult> registerDevice(String baseUrl) async {
  final dio = Dio(BaseOptions(baseUrl: baseUrl));
  final idempotencyKey = const Uuid().v4();

  final response = await dio.post<Map<String, dynamic>>(
    '/api/v1/auth/register-device',
    data: {
      'device_name': _deviceName(),
      'platform': Platform.operatingSystem,
      'client_version': '0.2.0',
    },
    options: Options(headers: {'Idempotency-Key': idempotencyKey}),
  );

  final token = response.data?['token'] as String?;
  if (token == null || token.isEmpty) {
    throw StateError('register-device: server returned no token');
  }
  final deviceId = response.data?['device_id'] as String? ?? '';
  return RegisterResult(token: token, deviceId: deviceId);
}

String _deviceName() {
  if (Platform.isAndroid) return 'Android Device';
  if (Platform.isIOS) return 'iOS Device';
  return 'Unknown Device';
}
