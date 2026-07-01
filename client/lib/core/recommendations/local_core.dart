import 'dart:async';
import 'dart:convert';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:path/path.dart' as p;
import 'package:path_provider/path_provider.dart';

import '../api/api_client.dart';
import '../api/sunflower_api.dart';
import '../auth/token_store.dart';
import '../bridge/api.dart' as bridge;
import '../bridge/frb_generated.dart';
import '../player/playback_feedback_recorder.dart';
import '../sync/sync_providers.dart';

final localCoreHandleProvider = FutureProvider<bridge.CoreHandle?>((ref) async {
  try {
    final recommendationServerUrl =
        ref.watch(recommendationServerUrlProvider).valueOrNull;
    await RustLib.init();
    final dir = await getApplicationSupportDirectory();
    final path = p.join(dir.path, 'sunflower-core.sqlite');
    return bridge.openCore(
      config: bridge.CoreConfig(
        sqlitePath: path,
        recommendationServerUrl: recommendationServerUrl,
      ),
    );
  } catch (_) {
    return null;
  }
});

final localRecommendationRecorderProvider =
    FutureProvider<LocalRecommendationRecorder?>((ref) async {
  final handle = await ref.watch(localCoreHandleProvider.future);
  if (handle == null) return null;
  return BridgeLocalRecommendationRecorder(handle);
});

final localFeedbackSyncProvider = Provider<void>((ref) {
  final localMode = ref.watch(localModeProvider).valueOrNull ?? false;
  final token = ref.watch(tokenProvider).valueOrNull ?? '';
  final baseUrl = ref.watch(recommendationBaseUrlProvider);
  if (localMode || token.isEmpty || baseUrl.isEmpty) {
    return;
  }

  Future<void> drain() async {
    try {
      final recorder =
          await ref.read(localRecommendationRecorderProvider.future);
      if (recorder is BridgeLocalRecommendationRecorder) {
        await recorder.drainFeedbackToServer(
          ref.read(recommendationFeedbackClientProvider),
        );
      }
    } catch (_) {
      // Local feedback replay is advisory; unsynced events stay in Rust SQLite.
    }
  }

  unawaited(drain());
  final timer = Timer.periodic(
    const Duration(seconds: 30),
    (_) => unawaited(drain()),
  );
  ref.onDispose(timer.cancel);
});

class BridgeLocalRecommendationRecorder implements LocalRecommendationRecorder {
  const BridgeLocalRecommendationRecorder(this._handle);

  final bridge.CoreHandle _handle;

  @override
  Future<String?> recordSongPlayback(
    Song song, {
    required String queueId,
    required DateTime occurredAt,
  }) async {
    await bridge.upsertLocalSong(
      handle: _handle,
      song: bridge.SongDto(
        mediaId: song.mediaId,
        sourceType: song.sourceType,
        title: song.title,
        artists: song.artistName.isEmpty ? const [] : [song.artistName],
        albumId: song.albumId,
        durationMs: song.durationMs,
        explicit: false,
        videoOnly: false,
        available: true,
        localPath: song.localPath,
      ),
    );
    final eventId = await _appendEvent(
      kind: bridge.RecommendationEventKindDto.playStarted,
      mediaId: song.mediaId,
      queueId: queueId,
      occurredAt: occurredAt,
      payload: {
        'title': song.title,
        if (song.durationMs != null) 'duration_ms': song.durationMs,
      },
    );
    return eventId;
  }

  @override
  Future<String?> recordStreamPlayback(
    ResolvedStream stream, {
    required String queueId,
    required DateTime occurredAt,
  }) async {
    final localPath = _localFilePath(stream.streamUrl);
    await bridge.upsertLocalSong(
      handle: _handle,
      song: bridge.SongDto(
        mediaId: stream.mediaId,
        sourceType: stream.source.isEmpty
            ? _sourceFromMediaId(stream.mediaId)
            : stream.source,
        title: stream.title.isEmpty ? stream.mediaId : stream.title,
        artists: stream.artists,
        durationMs: stream.durationMs <= 0 ? null : stream.durationMs,
        explicit: false,
        videoOnly: false,
        available: true,
        localPath: localPath,
      ),
    );
    final eventId = await _appendEvent(
      kind: bridge.RecommendationEventKindDto.playStarted,
      mediaId: stream.mediaId,
      queueId: queueId,
      occurredAt: occurredAt,
      payload: {
        if (stream.title.isNotEmpty) 'title': stream.title,
        if (stream.durationMs > 0) 'duration_ms': stream.durationMs,
      },
    );
    return eventId;
  }

  @override
  Future<String?> recordCompletion({
    required String mediaId,
    required String queueId,
    required int totalPlayedMs,
    required int durationMs,
    required DateTime occurredAt,
    String? eventId,
  }) {
    return _appendEvent(
      kind: bridge.RecommendationEventKindDto.playCompleted,
      mediaId: mediaId,
      queueId: queueId,
      occurredAt: occurredAt,
      eventId: eventId,
      payload: {
        'total_played_ms': totalPlayedMs,
        'duration_ms': durationMs,
      },
    );
  }

  @override
  Future<String?> recordPreference({
    required String mediaId,
    required bool liked,
    required DateTime occurredAt,
    String? eventId,
  }) {
    return _appendEvent(
      kind: liked
          ? bridge.RecommendationEventKindDto.liked
          : bridge.RecommendationEventKindDto.disliked,
      mediaId: mediaId,
      queueId: '',
      occurredAt: occurredAt,
      eventId: eventId,
      payload: {'liked': liked},
    );
  }

  @override
  Future<String?> recordImpression({
    required String sectionId,
    required String source,
    required String seedId,
    required String mediaId,
    required int position,
    required DateTime occurredAt,
    String? eventId,
  }) {
    return _appendEvent(
      kind: bridge.RecommendationEventKindDto.impression,
      mediaId: mediaId,
      queueId: '',
      occurredAt: occurredAt,
      eventId: eventId,
      payload: {
        'section_id': sectionId,
        'source': source,
        'seed_id': seedId,
        'position': position,
      },
    );
  }

  @override
  Future<void> markFeedbackQueued(List<String> eventIds) {
    if (eventIds.isEmpty) return Future.value();
    return bridge.markRecommendationEventsSynced(
      handle: _handle,
      eventIds: eventIds,
    );
  }

  Future<int> drainFeedbackToServer(
    RecommendationFeedbackClient api, {
    int limit = 100,
  }) async {
    final events = await bridge.unsyncedRecommendationEvents(
      handle: _handle,
      limit: limit,
    );
    final queued = <String>[];
    for (final event in events) {
      try {
        if (!_hasRemoteFeedbackContract(event)) {
          queued.add(event.eventId);
          continue;
        }
        await _enqueueRemoteFeedback(api, event);
        queued.add(event.eventId);
      } catch (_) {
        break;
      }
    }
    await markFeedbackQueued(queued);
    return queued.length;
  }

  Future<String> _appendEvent({
    required bridge.RecommendationEventKindDto kind,
    required String mediaId,
    required String queueId,
    required DateTime occurredAt,
    required Map<String, Object?> payload,
    String? eventId,
  }) async {
    final id = eventId ?? await bridge.newEventId();
    await bridge.appendRecommendationEvent(
      handle: _handle,
      event: bridge.RecommendationEventDto(
        eventId: id,
        deviceId: null,
        clientClock: occurredAt.toUtc().millisecondsSinceEpoch,
        occurredAtMs: occurredAt.toUtc().millisecondsSinceEpoch,
        kind: kind,
        mediaId: mediaId,
        queueId: queueId.isEmpty ? null : queueId,
        recommenderSource: bridge.RecommendationSourceDto.local,
        contextJson: '{}',
        payloadJson: jsonEncode(payload),
      ),
    );
    return id;
  }
}

Future<void> _enqueueRemoteFeedback(
  RecommendationFeedbackClient api,
  bridge.RecommendationEventDto event,
) async {
  final occurredAt = DateTime.fromMillisecondsSinceEpoch(
    event.occurredAtMs,
    isUtc: true,
  );
  final payload = _decodeObject(event.payloadJson);
  switch (event.kind) {
    case bridge.RecommendationEventKindDto.playCompleted:
      await api.playbackEvent(
        kind: 'play',
        mediaId: event.mediaId,
        queueId: event.queueId ?? '',
        totalPlayedMs: _intPayload(payload, 'total_played_ms') ??
            _intPayload(payload, 'duration_ms') ??
            0,
        durationMs: _intPayload(payload, 'duration_ms') ?? 0,
        occurredAt: occurredAt,
        idempotencyKey: event.eventId,
      );
    case bridge.RecommendationEventKindDto.skipped:
      await api.playbackEvent(
        kind: 'skip',
        mediaId: event.mediaId,
        queueId: event.queueId ?? '',
        totalPlayedMs: _intPayload(payload, 'total_played_ms') ?? 0,
        durationMs: _intPayload(payload, 'duration_ms') ?? 0,
        reason: payload['reason'] as String? ?? '',
        occurredAt: occurredAt,
        idempotencyKey: event.eventId,
      );
    case bridge.RecommendationEventKindDto.playStarted:
      return;
    case bridge.RecommendationEventKindDto.liked:
    case bridge.RecommendationEventKindDto.disliked:
      await api.like(
        event.mediaId,
        liked: event.kind == bridge.RecommendationEventKindDto.liked,
        occurredAt: occurredAt,
        idempotencyKey: event.eventId,
      );
    case bridge.RecommendationEventKindDto.impression:
      await api.logImpressions(
        [
          {
            'section_id': payload['section_id'] ?? '',
            'source': payload['source'] ?? '',
            'seed_id': payload['seed_id'] ?? '',
            'media_id': event.mediaId,
            'position': _intPayload(payload, 'position') ?? 0,
          },
        ],
        idempotencyKey: event.eventId,
      );
  }
}

bool _hasRemoteFeedbackContract(bridge.RecommendationEventDto event) {
  return event.kind != bridge.RecommendationEventKindDto.playStarted;
}

Map<String, Object?> _decodeObject(String raw) {
  if (raw.trim().isEmpty) return const {};
  final decoded = jsonDecode(raw);
  if (decoded is Map) return decoded.cast<String, Object?>();
  return const {};
}

int? _intPayload(Map<String, Object?> payload, String key) {
  final value = payload[key];
  if (value is int) return value;
  if (value is num) return value.toInt();
  if (value is String) return int.tryParse(value);
  return null;
}

String _sourceFromMediaId(String mediaId) {
  final index = mediaId.indexOf(':');
  return index <= 0 ? 'local' : mediaId.substring(0, index);
}

String? _localFilePath(String url) {
  final uri = Uri.tryParse(url);
  if (uri == null || uri.scheme != 'file') return null;
  return uri.toFilePath();
}
