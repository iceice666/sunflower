import 'dart:io';

import 'package:dio/dio.dart';
import 'package:uuid/uuid.dart';

/// Registers this device with the Sunflower server at [baseUrl] and returns
/// the opaque bearer token.
///
/// Throws [DioException] on network errors, [StateError] if the server returns
/// a success status but the body has no token field.
Future<String> registerDevice(String baseUrl) async {
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
  return token;
}

String _deviceName() {
  if (Platform.isAndroid) return 'Android Device';
  if (Platform.isIOS) return 'iOS Device';
  return 'Unknown Device';
}
