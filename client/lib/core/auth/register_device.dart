import 'dart:io';

import 'package:dio/dio.dart';
import 'package:uuid/uuid.dart';

import 'auth_failure.dart';

class SetupStatus {
  const SetupStatus({
    required this.configured,
    required this.pairingRequired,
    required this.serverVersion,
    required this.serverCapabilities,
  });

  final bool configured;
  final bool pairingRequired;
  final String serverVersion;
  final List<String> serverCapabilities;

  factory SetupStatus.fromJson(Map<String, dynamic> json) {
    return SetupStatus(
      configured: json['configured'] as bool? ?? false,
      pairingRequired: json['pairing_required'] as bool? ?? true,
      serverVersion: json['server_version'] as String? ?? '',
      serverCapabilities:
          (json['server_capabilities'] as List<dynamic>? ?? const [])
              .cast<String>(),
    );
  }
}

/// Result of device registration: the opaque bearer token and the server's
/// device id (needed for the M6 per-device download registry).
class RegisterResult {
  const RegisterResult({required this.token, required this.deviceId});
  final String token;
  final String deviceId;
}

Future<SetupStatus> fetchSetupStatus(String baseUrl) async {
  final dio = Dio(BaseOptions(baseUrl: baseUrl));
  final response = await dio.get<Map<String, dynamic>>(
    '/api/v1/setup/status',
  );
  return SetupStatus.fromJson(response.data ?? const {});
}

/// Pairs this device with the Sunflower server at [baseUrl] and returns the
/// opaque bearer token plus the assigned device id.
///
/// Throws [DioException] on network errors, [StateError] if the server returns
/// a success status but the body has no token field.
Future<RegisterResult> registerDevice(
  String baseUrl, {
  required String pairingCode,
}) async {
  final dio = Dio(BaseOptions(baseUrl: baseUrl));
  final idempotencyKey = const Uuid().v4();

  late final Response<Map<String, dynamic>> response;
  try {
    response = await dio.post<Map<String, dynamic>>(
      '/api/v1/auth/register-device',
      data: {
        'device_name': _deviceName(),
        'platform': Platform.operatingSystem,
        'client_version': '0.3.0',
        'pairing_code': pairingCode,
      },
      options: Options(headers: {'Idempotency-Key': idempotencyKey}),
    );
  } catch (e) {
    throw classifyAuthFailure(e);
  }

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
