import 'package:dio/dio.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../api/api_client.dart';
import '../auth/auth_failure.dart';
import '../auth/token_store.dart';
import '../db/database_provider.dart';
import 'replay_buffer.dart';

/// A Dio configured for the replay buffer: base URL + bearer token from stored
/// credentials. Separate from SunflowerApi's Dio so the buffer can attach its
/// own Idempotency-Key header per request.
final _replayDioProvider = Provider<Dio>((ref) {
  final baseUrl = ref.watch(serverUrlProvider).valueOrNull ?? '';
  return _authedDio(ref, baseUrl);
});

/// Dio for standalone recommendation feedback. It falls back to the main
/// server when no separate recommendation server is configured.
final _recommendationFeedbackDioProvider = Provider<Dio>((ref) {
  return _authedDio(ref, ref.watch(recommendationBaseUrlProvider));
});

/// The singleton write-replay buffer. Started on first read.
final replayBufferProvider = Provider<ReplayBuffer>((ref) {
  final buffer = ReplayBuffer(
    dio: ref.watch(_replayDioProvider),
    db: ref.watch(databaseProvider),
  );
  buffer.start();
  ref.onDispose(buffer.dispose);
  return buffer;
});

/// The buffered mutation API the UI uses for all writes.
final bufferedApiProvider = Provider<BufferedApiClient>((ref) {
  return BufferedApiClient(ref.watch(replayBufferProvider));
});

/// Direct feedback sink for the local recommender event log.
final recommendationFeedbackClientProvider =
    Provider<RecommendationFeedbackClient>((ref) {
  return DirectRecommendationFeedbackClient(
    dio: ref.watch(_recommendationFeedbackDioProvider),
  );
});

/// Live count of unconfirmed pending mutations.
final pendingCountProvider = StreamProvider<int>((ref) {
  return ref.watch(replayBufferProvider).watchPendingCount();
});

Dio _authedDio(Ref ref, String baseUrl) {
  final token = ref.watch(tokenProvider).valueOrNull ?? '';
  final dio = Dio(BaseOptions(
    baseUrl: baseUrl,
    connectTimeout: const Duration(seconds: 10),
    receiveTimeout: const Duration(seconds: 30),
  ));
  if (token.isNotEmpty) {
    dio.options.headers['Authorization'] = 'Bearer $token';
    dio.interceptors.add(
      InterceptorsWrapper(
        onError: (error, handler) async {
          final failure = classifyAuthFailure(error);
          if (failure.kind == AuthFailureKind.missingToken ||
              failure.kind == AuthFailureKind.invalidToken ||
              failure.kind == AuthFailureKind.deviceRevoked) {
            await clearCredentialsAndNotify(ref);
          }
          handler.next(error);
        },
      ),
    );
  }
  return dio;
}
