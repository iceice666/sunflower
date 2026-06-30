import 'package:dio/dio.dart';

enum AuthFailureKind {
  missingToken,
  invalidToken,
  deviceRevoked,
  pairingRequired,
  invalidPairingCode,
  rateLimited,
  unreachable,
  unknown,
}

class AuthFailure implements Exception {
  const AuthFailure(this.kind, this.message);

  final AuthFailureKind kind;
  final String message;

  @override
  String toString() => message;
}

AuthFailure classifyAuthFailure(Object error) {
  if (error is DioException) {
    final status = error.response?.statusCode;
    final code = _serverCode(error.response?.data);
    switch (code) {
      case 'missing_token':
        return const AuthFailure(
          AuthFailureKind.missingToken,
          'Sign in again to continue.',
        );
      case 'invalid_token':
        return const AuthFailure(
          AuthFailureKind.invalidToken,
          'This device token is no longer valid.',
        );
      case 'device_revoked':
        return const AuthFailure(
          AuthFailureKind.deviceRevoked,
          'This device was revoked. Pair it again from the admin dashboard.',
        );
      case 'pairing_required':
        return const AuthFailure(
          AuthFailureKind.pairingRequired,
          'Enter a pairing code from the admin dashboard.',
        );
      case 'invalid_pairing_code':
        return const AuthFailure(
          AuthFailureKind.invalidPairingCode,
          'That pairing code is invalid, expired, or already used.',
        );
      case 'rate_limited':
        return const AuthFailure(
          AuthFailureKind.rateLimited,
          'Too many attempts. Wait a moment and try again.',
        );
    }
    if (status == 401 || status == 403) {
      return const AuthFailure(
        AuthFailureKind.unknown,
        'Authentication failed.',
      );
    }
    if (error.type == DioExceptionType.connectionError ||
        error.type == DioExceptionType.connectionTimeout ||
        error.type == DioExceptionType.receiveTimeout) {
      return const AuthFailure(
        AuthFailureKind.unreachable,
        'Server is not reachable.',
      );
    }
  }
  return AuthFailure(AuthFailureKind.unknown, error.toString());
}

String? _serverCode(Object? data) {
  if (data is Map && data['error'] is String) {
    return data['error'] as String;
  }
  return null;
}
