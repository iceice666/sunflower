import 'package:dio/dio.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sunflower/core/auth/auth_failure.dart';

void main() {
  test('classifies JSON map auth errors', () {
    final failure = classifyAuthFailure(_dioError(
      statusCode: 403,
      data: {'error': 'pairing_required'},
    ));

    expect(failure.kind, AuthFailureKind.pairingRequired);
  });

  test('classifies text/plain JSON auth errors from legacy middleware', () {
    final failure = classifyAuthFailure(_dioError(
      statusCode: 401,
      data: '{"error":"invalid_token"}\n',
    ));

    expect(failure.kind, AuthFailureKind.invalidToken);
  });

  test('falls back for unstructured 401 bodies', () {
    final failure = classifyAuthFailure(_dioError(
      statusCode: 401,
      data: 'unauthorized',
    ));

    expect(failure.kind, AuthFailureKind.unknown);
  });
}

DioException _dioError({
  required int statusCode,
  required Object? data,
}) {
  final requestOptions = RequestOptions(path: '/api/v1/library/songs');
  return DioException.badResponse(
    statusCode: statusCode,
    requestOptions: requestOptions,
    response: Response(
      requestOptions: requestOptions,
      statusCode: statusCode,
      data: data,
    ),
  );
}
